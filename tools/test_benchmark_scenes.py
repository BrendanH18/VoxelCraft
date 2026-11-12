#!/usr/bin/env python3
"""Unit tests for the live scene runner's gate logic: python3 -m unittest tools/test_benchmark_scenes.py"""
import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0,str(Path(__file__).resolve().parent))
import benchmark_scenes as runner

SPEC=json.loads((runner.ROOT/'benchmarks/scenes.json').read_text())
DEFINITIONS={s['name']:s for s in SPEC['scenes']}

def report(scene='survival',**changes):
    seconds=180.0 if scene=='streaming' else 60.0
    out={'scene':scene,'sample_seconds':seconds+0.01,'completed_simulation_ticks':int(seconds*20),'paused_simulation_ms':0.0,
         'unpresented_frames':0,'graphics':'enhanced','acceptance_eligible_configuration':True,'run_valid':True,
         'ineligible_reasons':[],'source_matches_runner':True,'frame_thresholds_passed':True,
         'view_distance_blocks':SPEC['view_distance_blocks'],'simulation_distance_blocks':SPEC['simulation_distance_blocks']}
    out.update(changes)
    return out

def summary(runs,chip='Apple M5'):
    for r in runs: r['gate_problems']=runner.run_problems(r,SPEC)
    return {'hardware':{'chip':chip},'runs':runs}

def full_suite(**changes):
    return [report(scene,**changes) for scene in DEFINITIONS for _ in range(3)]

class RunnerGate(unittest.TestCase):
    def test_valid_suite_passes(self):
        passed,problems=runner.reference_gate(summary(full_suite()),DEFINITIONS,False,None,3)
        self.assertTrue(passed,problems)

    def test_recorded_paused_streaming_run_is_rejected(self):
        # benchmarks/results/2026-10-13-94a34dd-m5 streaming run 3: 180 s sampled, zero ticks.
        paused=report('streaming',completed_simulation_ticks=0,sample_seconds=180.01)
        self.assertIn('completed simulation ticks differ from 20 per sampled second',runner.run_problems(paused,SPEC))
        # Schema-1 reports carry no pause or presentation telemetry and cannot pass.
        legacy={k:v for k,v in report().items() if k not in ('paused_simulation_ms','unpresented_frames','run_valid')}
        self.assertTrue(runner.run_problems(legacy,SPEC))
        runs=full_suite();last=max(i for i,r in enumerate(runs) if r['scene']=='streaming');runs[last]=paused
        passed,problems=runner.reference_gate(summary(runs),DEFINITIONS,False,None,3)
        self.assertFalse(passed)
        self.assertTrue(any(f'streaming-{last+1}' in p for p in problems),problems)

    def test_each_run_requirement(self):
        for change in [{'paused_simulation_ms':12.0},{'unpresented_frames':1},{'graphics':'classic'},
                       {'run_valid':False,'ineligible_reasons':['scene population drifted beyond tolerance']},
                       {'acceptance_eligible_configuration':False},{'source_matches_runner':False},
                       {'completed_simulation_ticks':1150},{'simulation_distance_blocks':128},
                       {'view_distance_blocks':128}]:
            self.assertTrue(runner.run_problems(report(**change),SPEC),change)
        self.assertEqual(runner.run_problems(report(completed_simulation_ticks=1201),SPEC),[])

    def test_menu_scenes_pause_and_the_title_has_no_world(self):
        menus=report('menus',completed_simulation_ticks=310,paused_simulation_ms=44_500.0)
        self.assertEqual(runner.run_problems(menus,SPEC),[])
        title=report('title',completed_simulation_ticks=0,paused_simulation_ms=None,view_distance_blocks=None,simulation_distance_blocks=None)
        self.assertEqual(runner.run_problems(title,SPEC),[])
        self.assertTrue(runner.run_problems(report('menus',view_distance_blocks=128),SPEC))
        self.assertTrue(runner.run_problems(report('title',unpresented_frames=2),SPEC))

    def test_protocol_requirements(self):
        runs=summary(full_suite())
        self.assertFalse(runner.reference_gate(runs,DEFINITIONS,True,None,3)[0])
        self.assertFalse(runner.reference_gate(runs,DEFINITIONS,False,'3024x1964',3)[0])
        self.assertFalse(runner.reference_gate(runs,DEFINITIONS,False,None,2)[0])
        self.assertFalse(runner.reference_gate(summary(full_suite(),chip='Apple M4'),DEFINITIONS,False,None,3)[0])
        self.assertFalse(runner.reference_gate(summary(full_suite(frame_thresholds_passed=False)),DEFINITIONS,False,None,3)[0])
        self.assertFalse(runner.reference_gate(summary(full_suite()[:-3]),DEFINITIONS,False,None,3)[0])
        # Comparison runs: drawn offscreen, or with a shortened sample period.
        self.assertIn('offscreen comparison runs present nothing',runner.reference_gate(runs,DEFINITIONS,False,None,3,offscreen=True)[1])
        self.assertFalse(runner.reference_gate(runs,DEFINITIONS,False,None,3,seconds=30.0)[0])

    def test_presentation_experiments_are_recorded_and_never_pass(self):
        # The defaults add no flags, so a plain run's command is unchanged.
        self.assertEqual(runner.presentation(),([],{'frame_latency':2,'window_mode':'windowed','gpu_timestamps':True},[]))
        self.assertEqual(runner.presentation(2,'windowed')[0],[])
        flags,options,differences=runner.presentation(3,'borderless',False)
        self.assertEqual(flags,['--benchmark-frame-latency','3','--benchmark-window-mode','borderless','--benchmark-no-gpu-timestamps'])
        self.assertEqual(options,{'frame_latency':3,'window_mode':'borderless','gpu_timestamps':False})
        self.assertEqual(differences,['frame latency 3','window mode borderless'])
        # Turning GPU timing off is a control, not a different configuration.
        self.assertEqual(runner.presentation(None,None,False)[2],[])
        runs=summary(full_suite())
        passed,problems=runner.reference_gate(runs,DEFINITIONS,False,None,3,presentation_differences=differences)
        self.assertFalse(passed)
        self.assertIn('presentation experiment: frame latency 3, window mode borderless',problems)
        # A fullscreen run on a display that isn't 1920x1080 reports the
        # game's configuration problem, so the run itself is ineligible too.
        native=report(acceptance_eligible_configuration=False,ineligible_reasons=['resolution is not 1920x1080'])
        self.assertIn('resolution is not 1920x1080',runner.run_problems(native,SPEC))

    def test_runner_constants_match_the_game(self):
        source=(runner.ROOT/'src/performance.rs').read_text()
        self.assertIn(f'pub const DEFAULT_FRAME_LATENCY: u32 = {runner.DEFAULT_FRAME_LATENCY};',source)
        modes=', '.join(f'"{m}"' for m in runner.WINDOW_MODES)
        self.assertIn(f'pub const WINDOW_MODES: [&str; {len(runner.WINDOW_MODES)}] = [{modes}];',source)
        self.assertIn(f'pub const TICK_TOLERANCE: f64 = {runner.TICK_TOLERANCE};',source)

    def test_gpu_line_summarises_slow_acquires(self):
        self.assertIsNone(runner.gpu_line({'gpu_timestamp_ms':None}))
        group=lambda frame,acquire,gpu:{'frames':10,'timed_frames':10,'frame_ms':frame,'acquire_ms':acquire,'gpu_ms':gpu}
        line=runner.gpu_line({'gpu_timestamp_ms':{'slow_acquire_threshold_ms':4.0,
            'all_frames':group(2.0,1.3,{'mean':1.1,'p99':1.6}),'slowest_one_percent':group(8.8,7.6,{'mean':1.2,'p99':1.4}),
            'slow_acquire_frames':group(8.6,7.4,{'mean':1.25,'p99':1.5}),'other_frames':group(1.9,1.2,None)}})
        self.assertIn('slowest 1% of frames: GPU 1.200 of 8.800 ms (acquire 7.600)',line)
        self.assertIn('10 frames acquired over 4 ms: GPU mean 1.250, p99 1.500',line)

    def test_fingerprint_includes_build_embedded_inputs(self):
        paths={p.relative_to(runner.ROOT).as_posix() for p in runner.source_paths()}
        for required in ['build.rs','packaging/icons/VoxelCraft.ico','packaging/icons/VoxelCraft.png',
                         'tests/fixtures/java/26.3/mining.json','assets/manifest.json','src/main.rs']:
            self.assertIn(required,paths)

if __name__=='__main__':unittest.main()
