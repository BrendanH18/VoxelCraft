#!/usr/bin/env python3
"""Run repeatable live-game scenes and record source, hardware, frames, RSS and gate results."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import time

ROOT=Path(__file__).resolve().parents[1]

def command(*args):
    return subprocess.check_output(args,cwd=ROOT,text=True).strip()

def source():
    paths=[ROOT/'Cargo.toml',ROOT/'Cargo.lock',ROOT/'build.rs']
    for directory in ['src','assets']:
        paths.extend(p for p in (ROOT/directory).rglob('*') if p.is_file())
    digest=hashlib.sha256()
    for path in sorted(paths): digest.update(path.relative_to(ROOT).as_posix().encode()+b'\0'+path.read_bytes()+b'\0')
    return {'commit':command('git','rev-parse','HEAD'),'dirty':bool(command('git','status','--porcelain')),'runtime_source_sha256':digest.hexdigest()}

def hardware():
    out={'platform':platform.platform(),'architecture':platform.machine(),'logical_cpus':os.cpu_count()}
    if platform.system()=='Darwin':
        out['chip']=command('sysctl','-n','machdep.cpu.brand_string')
        out['ram_bytes']=int(command('sysctl','-n','hw.memsize'))
        displays=json.loads(command('system_profiler','SPDisplaysDataType','-json'))['SPDisplaysDataType']
        out['graphics']=[{'chip':gpu.get('sppci_model'),'cores':gpu.get('sppci_cores'),'displays':[{'resolution':d.get('spdisplays_resolution'),'native_pixels':d.get('_spdisplays_pixels')} for d in gpu.get('spdisplays_ndrvs',[])]} for gpu in displays]
    else: out['chip']=platform.processor()
    return out

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--executable',type=Path)
    parser.add_argument('--output',type=Path,default=ROOT/'target/benchmarks')
    parser.add_argument('--scenes',default='survival,building,entities,automation,streaming')
    parser.add_argument('--repeats',type=int,default=3)
    parser.add_argument('--quick',action='store_true',help='five-second smoke only, never acceptance evidence')
    parser.add_argument('--native-resolution',help='WIDTHxHEIGHT, reported separately from the 1080p gate')
    args=parser.parse_args()
    assert 1<=args.repeats<=10,'repeat count must be 1..10'
    specification=json.loads((ROOT/'benchmarks/scenes.json').read_text())
    definitions={s['name']:s for s in specification['scenes']}
    names=args.scenes.split(',')
    assert all(n in definitions for n in names),'unknown scene'
    revision=source()
    if args.executable:
        executable=args.executable.resolve(strict=True)
    else:
        env=os.environ.copy();env['VOXELCRAFT_BUILD_REVISION']=revision['runtime_source_sha256']
        subprocess.run(['cargo','build','--release','--bin','voxelcraft'],cwd=ROOT,env=env,check=True)
        executable=Path(env.get('CARGO_TARGET_DIR',ROOT/'target'))/'release/voxelcraft'
    executable=executable.resolve(strict=True)
    args.output.mkdir(parents=True,exist_ok=True)
    metadata={'schema':1,'source':revision,'hardware':hardware(),'executable_sha256':hashlib.sha256(executable.read_bytes()).hexdigest(),
              'protocol':'quick smoke' if args.quick else 'scene baseline','resolution_class':'native' if args.native_resolution else '1080p','specification':specification,'runs':[]}
    resolution=args.native_resolution or '1920x1080'
    for scene in names:
        for run in range(args.repeats):
            output=args.output/f'{scene}-{run+1}.json'
            duration=5 if args.quick else definitions[scene].get('sample_seconds',specification['sample_seconds'])
            warmup=1 if args.quick else specification['warmup_seconds']
            cmd=[str(executable),'--benchmark-scene',scene,'--benchmark-output',str(output.resolve()),'--benchmark-duration',str(duration),'--benchmark-warmup',str(warmup),
                 '--size',resolution,'--seed',str(specification['seed']),'--rd','8','--graphics',specification['graphics'],'--no-vsync','--new','--world','benchmark',
                 '--data-dir',str((args.output/'player-data').resolve())]
            print(f'{scene} run {run+1}/{args.repeats} at {resolution}',flush=True)
            peak_rss_kib=0;started=time.monotonic()
            with (args.output/f'{scene}-{run+1}.log').open('w') as log:
                process=subprocess.Popen(cmd,cwd=ROOT,stdout=log,stderr=subprocess.STDOUT)
                try:
                    while process.poll() is None:
                        if time.monotonic()-started>duration+warmup+240: raise TimeoutError(f'{scene}: process exceeded fixture deadline')
                        if platform.system() in ('Darwin','Linux'):
                            rss=subprocess.run(['ps','-o','rss=','-p',str(process.pid)],text=True,capture_output=True)
                            if rss.stdout.strip(): peak_rss_kib=max(peak_rss_kib,int(rss.stdout.strip()))
                        time.sleep(0.5)
                    if process.returncode: raise RuntimeError(f'{scene}: exit {process.returncode}; inspect {log.name}')
                finally:
                    if process.poll() is None: process.terminate();process.wait(timeout=10)
            report=json.loads(output.read_text());report['peak_process_rss_bytes']=peak_rss_kib*1024
            report['wall_elapsed_seconds']=time.monotonic()-started
            # A supplied binary may have been built from different working source: do not misattribute it.
            report['source_matches_runner']=report['source_revision']==revision['runtime_source_sha256']
            output.write_text(json.dumps(report,indent=2)+'\n')
            metadata['runs'].append({k:v for k,v in report.items() if k!='frame_times_ms'})
            print(f"  {report['frame_stats']['one_percent_low_fps']:.1f} FPS 1% low, {report['frame_stats']['p99_ms']:.3f} ms p99; peak RSS {peak_rss_kib/1024:.1f} MiB",flush=True)
            (args.output/'summary.json').write_text(json.dumps(metadata,indent=2)+'\n')
    measured_names={r['scene'] for r in metadata['runs']}
    metadata['reference_gate_passed']=not args.quick and not args.native_resolution and args.repeats>=3 and measured_names==set(definitions) and 'M5' in metadata['hardware'].get('chip','') and all(r['acceptance_eligible_configuration'] and r['frame_thresholds_passed'] and r['source_matches_runner'] for r in metadata['runs'])
    metadata['remaining_acceptance']='These five initial scenes do not cover the full roadmap environment/encounter matrix or ordinary player testing. GPU timestamp costs and long-session memory stability remain separate checks.'
    (args.output/'summary.json').write_text(json.dumps(metadata,indent=2)+'\n')

if __name__=='__main__':main()
