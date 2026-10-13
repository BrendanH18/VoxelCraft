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
         'ineligible_reasons':[],'source_matches_runner':True,'frame_thresholds_passed':True}
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
        runs=full_suite();runs[-1]=paused
        passed,problems=runner.reference_gate(summary(runs),DEFINITIONS,False,None,3)
        self.assertFalse(passed)
        self.assertTrue(any('streaming-15' in p for p in problems),problems)

    def test_each_run_requirement(self):
        for change in [{'paused_simulation_ms':12.0},{'unpresented_frames':1},{'graphics':'classic'},
                       {'run_valid':False,'ineligible_reasons':['scene population drifted beyond tolerance']},
                       {'acceptance_eligible_configuration':False},{'source_matches_runner':False},
                       {'completed_simulation_ticks':1150}]:
            self.assertTrue(runner.run_problems(report(**change),SPEC),change)
        self.assertEqual(runner.run_problems(report(completed_simulation_ticks=1201),SPEC),[])

    def test_protocol_requirements(self):
        runs=summary(full_suite())
        self.assertFalse(runner.reference_gate(runs,DEFINITIONS,True,None,3)[0])
        self.assertFalse(runner.reference_gate(runs,DEFINITIONS,False,'3024x1964',3)[0])
        self.assertFalse(runner.reference_gate(runs,DEFINITIONS,False,None,2)[0])
        self.assertFalse(runner.reference_gate(summary(full_suite(),chip='Apple M4'),DEFINITIONS,False,None,3)[0])
        self.assertFalse(runner.reference_gate(summary(full_suite(frame_thresholds_passed=False)),DEFINITIONS,False,None,3)[0])
        self.assertFalse(runner.reference_gate(summary(full_suite()[:-3]),DEFINITIONS,False,None,3)[0])

    def test_fingerprint_includes_build_embedded_inputs(self):
        paths={p.relative_to(runner.ROOT).as_posix() for p in runner.source_paths()}
        for required in ['build.rs','packaging/icons/VoxelCraft.ico','packaging/icons/VoxelCraft.png',
                         'tests/fixtures/java/26.3/mining.json','assets/manifest.json','src/main.rs']:
            self.assertIn(required,paths)

if __name__=='__main__':unittest.main()
