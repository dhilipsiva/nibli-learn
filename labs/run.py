#!/usr/bin/env python3
"""Pinned local experiments. Reports describe supplied observations, not authenticated execution."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import socket
import threading
import tempfile
import time
import tomllib

ROOT = Path(__file__).resolve().parent
APP = ROOT.parent
ENGINE = '0440db4233f8a8cffbf81be9654a5e54c1f9c836'
NATIVE = ['compute', 'measurement', 'ontology', 'scoped-application', 'compiler-internals', 'store-search', 'rules-witnesses', 'runtime-buffers', 'finite-assurance']
LABS = NATIVE + ['wasi-runtime', 'formal-models']

def digest(data):
    return hashlib.sha256(data).hexdigest()

def inventory():
    files = {}
    for p in sorted(ROOT.rglob('*')):
        if p.is_file() and not p.is_symlink() and not any(s in {'target','__pycache__'} for s in p.relative_to(ROOT).parts):
            files[p.relative_to(ROOT).as_posix()] = digest(p.read_bytes())
    return files

def command(args, cwd, output, name, env=None, stdin=None, timeout=600):
    started = time.monotonic()
    with (output / (name+'.stdout')).open('w') as out, (output / (name+'.stderr')).open('w') as err:
        result = subprocess.run(args, cwd=cwd, env=env, input=stdin, text=True, stdout=out, stderr=err, timeout=timeout)
    if result.returncode:
        raise RuntimeError(f'{name} failed ({result.returncode}); inspect {name}.stderr and {name}.stdout')
    return {'command':args,'exit_code':result.returncode,'elapsed_seconds':time.monotonic()-started}

def tool(args, cwd=APP):
    return subprocess.check_output(args, cwd=cwd, text=True).strip()

def verify_engine(root):
    if tool(['git','rev-parse','HEAD'],root) != ENGINE:
        raise RuntimeError('Engine checkout does not match the pinned revision.')
    if tool(['git','status','--porcelain','--untracked-files=no'],root):
        raise RuntimeError('Engine tracked sources must be clean before building the experiment.')
    lock=tomllib.loads((root/'Cargo.lock').read_text())
    if not any(p['name']=='wasmtime' and p['version']=='49.0.5' for p in lock['package']):
        raise RuntimeError('Unexpected Wasmtime dependency.')

def native(name, output):
    lock=tomllib.loads((ROOT/'native/Cargo.lock').read_text())
    for p in lock['package']:
        if p['name'].startswith('nibli-') and p['name'] != 'nibli-learn-labs' and not p.get('source','').endswith('#'+ENGINE):
            raise RuntimeError('Unpinned engine dependency: '+p['name'])
    env=os.environ.copy();env['CARGO_TARGET_DIR']=str(APP/'target/learning-labs');env['NIBLI_BACKEND_READ_TIMEOUT_SECS']='1'
    # Strip inherited endpoint/DB settings; experiments own loopback endpoints and files.
    for key in ['NIBLI_DB_PATH','NIBLI_COMPUTE_ADDR','NIBLI_BACKEND_ADDR']:
        env.pop(key,None)
    build=command(['cargo','build','--locked','--release','--manifest-path',str(ROOT/'native/Cargo.toml')],APP,output,'build',env=env)
    binary=APP/'target/learning-labs/release/nibli-learn-labs'
    run=command([str(binary),name,str(output)],APP,output,'experiment',env=env,timeout=180)
    observed=json.loads((output/'experiment.stdout').read_text())
    if observed.get('engine_revision') != ENGINE or observed.get('lab_id') != name:
        raise RuntimeError('Native experiment returned an incompatible identity.')
    checks=observed['observations']['checks']
    if not checks or not all(v is True for v in checks.values()):
        raise RuntimeError('A native behavioral check failed.')
    return observed['observations'], {'native_binary_sha256':digest(binary.read_bytes())}, [build,run]

def wasi(output, engine):
    verify_engine(engine)
    build=command(['nix','develop','--command','just','build-wasm','build-host'],engine,output,'build',timeout=1800)
    verify_engine(engine)
    component=engine/'target/wasm32-wasip2/debug/nibli.wasm';host=engine/'target/debug/nibli-host'
    command(['nix','develop','--command','wasm-tools','component','wit',str(component)],engine,output,'interface')
    wit=(output/'interface.stdout').read_text();
    # nix shell diagnostics can precede tool output, so isolate the WIT world.
    assert 'nibli:engine@0.12.0' in wit, 'unexpected WIT package identity'
    imports=[line.strip() for line in wit.splitlines() if line.strip().startswith('import ')]
    assert any('compute-backend' in line for line in imports), 'compute host import missing'
    for denied in ['filesystem','sockets','http','clocks']:
        assert not any(denied in line for line in imports), 'unexpected guest capability: '+denied
    env=os.environ.copy();env.update(NIBLI_WASM_PATH=str(component),NIBLI_FUEL='50000000000',NIBLI_MEMORY_MB='512',NIBLI_BACKEND_READ_TIMEOUT_SECS='1')
    for key in ['NIBLI_DB_PATH','NIBLI_COMPUTE_ADDR','NIBLI_MAX_CHAIN_DEPTH']:env.pop(key,None)
    fuel_script='computer(Terminal7).\n:fuel 1000\n? computer(Terminal7).\n:fuel 50000000000\n? computer(Terminal7).\n:facts\n'
    fuel=command([str(host)],engine,output,'fuel-recovery',env=env,stdin=fuel_script,timeout=180)
    fuel_text=(output/'fuel-recovery.stdout').read_text()
    assert 'RESOURCE_EXCEEDED (fuel)' in fuel_text and '[Session] Rebuilding and replaying' in fuel_text and '[Query] TRUE' in fuel_text and '#0: computer(Terminal7).' in fuel_text, 'fuel recovery contract failed'
    # A one-MiB cap after initialization cannot shrink existing memory. A bounded
    # large query forces a subsequent growth request; observe the actual trap.
    memory_source='computer(Panel'+'x'*(2*1024*1024)+').'
    memory_script='computer(Terminal7).\n:memory 1\n? '+memory_source+'\n:memory 512\n? computer(Terminal7).\n:facts\n'
    memory=command([str(host)],engine,output,'memory-recovery',env=env,stdin=memory_script,timeout=180)
    memory_text=(output/'memory-recovery.stdout').read_text()
    assert 'RESOURCE_EXCEEDED (memory)' in memory_text and '[Session] Rebuilding and replaying' in memory_text and '[Query] TRUE' in memory_text and '#0: computer(Terminal7).' in memory_text, 'memory recovery contract failed'
    env['NIBLI_DB_PATH']=str(output/'wasi-journal.redb')
    setup='computer(Terminal7).\ncomputer(Terminal8).\ncomputer(Terminal9).\n:retract 1\n'
    journal1=command([str(host)],engine,output,'journal-write',env=env,stdin=setup,timeout=180)
    journal2=command([str(host)],engine,output,'journal-reopen',env=env,stdin=':facts\n:retract 2\n? computer(Terminal9).\n? computer(Terminal7).\n',timeout=180)
    reopened=(output/'journal-reopen.stdout').read_text()
    assert '#2: computer(Terminal9).' in reopened and '#1:' not in reopened and '[Query] FALSE' in reopened and '[Query] TRUE' in reopened, 'journal identity or withdrawal contract failed'
    env.pop('NIBLI_DB_PATH')
    parity='\n'.join((ROOT/'fixtures/scoped-inspection.nibli').read_text().splitlines())+'\n? ready(CartB, CaseA).\napproves(Reviewer, CaseA).\n? ready(CartB, CaseA).\n'
    application=command([str(host)],engine,output,'scoped-application',env=env,stdin=parity,timeout=180)
    parity_text=(output/'scoped-application.stdout').read_text();assert '[Query] FALSE' in parity_text and '[Query] TRUE' in parity_text
    requests=[]; failures=[]
    listener=socket.socket();listener.bind(('127.0.0.1',0));listener.listen();listener.settimeout(30)
    def backend():
        try:
            conn,_=listener.accept()
            with conn:
                conn.settimeout(30)
                stream=conn.makefile('rwb')
                for _ in range(2):
                    request=json.loads(stream.readline());requests.append(request)
                    assert request['relation']=='exponential'
                    result,base,power=[arg['value'] for arg in request['args']]
                    stream.write((json.dumps({'result':result==base**power})+'\n').encode());stream.flush()
        except Exception as error:failures.append(str(error))
        finally:listener.close()
    service=threading.Thread(target=backend,daemon=True);service.start()
    env['NIBLI_COMPUTE_ADDR']='127.0.0.1:'+str(listener.getsockname()[1])
    compute=command([str(host)],engine,output,'external-compute',env=env,stdin=':compute exponential\n? exponential(8, 2, 3).\n? exponential(9, 2, 3).\n:facts\n',timeout=180)
    service.join(timeout=35)
    assert not service.is_alive() and not failures and len(requests)==2, 'WASI backend dispatch did not complete'
    compute_text=(output/'external-compute.stdout').read_text()
    assert '[Query] TRUE' in compute_text and '[Query] FALSE' in compute_text and '[Facts] Knowledge base is empty.' in compute_text, 'WASI compute result or durability control failed'
    keep=lambda text:[line for line in text.splitlines() if line.startswith(('[Query]','[Session]','[Fact #','[Retract]','[Memory]','[Fuel]','  #'))]
    observations={'external_compute':{'requests':requests,'transcript':keep(compute_text)},'imports':imports,'exports':[line.strip() for line in wit.splitlines() if line.strip().startswith('export ')],'wit_identity':'nibli:engine@0.12.0','host_wasmtime_version':'49.0.5','fuel':keep(fuel_text),'memory':keep(memory_text),'memory_query_bytes':len(memory_source.encode()),'journal':keep(reopened),'scoped_application':keep(parity_text),'checks':{'external_compute_is_query_local':True,'component_identity_and_imports':True,'fuel_trap_then_recovery':True,'memory_trap_then_recovery':True,'stable_journal_ids_with_tombstone':True,'scoped_application_native_wasi_outcomes':True},'limitations':['Host has filesystem and loopback capabilities; absence of guest imports does not sandbox the host.','Fuel counts executed WASM work; it is not elapsed time or reasoner depth.','Memory cap applies to component growth, not whole-process RSS.','Authorizer exports are not exercised by this lab.']}
    artifacts={'component_sha256':digest(component.read_bytes()),'host_sha256':digest(host.read_bytes()),'wit_sha256':digest((output/'interface.stdout').read_bytes()),'engine_lock_sha256':digest((engine/'Cargo.lock').read_bytes())}
    return observations, artifacts, [build,fuel,memory,journal1,journal2,application,compute]

def formal_models(output, engine):
    verify_engine(engine)
    version = command(['nix','develop','--command','lean','--version'],engine,output,'lean-version')
    lean_version = next((line for line in (output/'lean-version.stdout').read_text().splitlines() if line.startswith('Lean (version ')), '')
    assert lean_version.startswith('Lean (version '), 'Lean must actually be installed; skipping is not success'
    checked = command(['nix','develop','--command','just','verify-proofs'],engine,output,'lean-models')
    transcript = (output/'lean-models.stdout').read_text()
    assert 'all proofs/*.lean check' in transcript and 'skipping' not in transcript
    files = sorted(engine.glob('proofs/*.lean'))
    assert files and all('checking '+p.relative_to(engine).as_posix() in transcript for p in files)
    cap_test = 'proof_cap_aborts_quantified_rewalk_without_publishing_partial_certificate'
    cap_check = command(['nix','develop','--command','cargo','test','--locked','-p','nibli-reason','--lib',cap_test],engine,output,'proof-cap-control')
    cap_text = (output/'proof-cap-control.stdout').read_text()
    assert cap_test in cap_text and '1 passed; 0 failed' in cap_text, 'the proof-cap control must actually execute'
    verify_engine(engine)
    inventory_path = engine/'proofs/self-check/coverage.toml'
    coverage = tomllib.loads(inventory_path.read_text())
    extraction_path = engine/'proofs/self-check/extraction.toml'
    extraction = tomllib.loads(extraction_path.read_text())
    assert coverage['certified_target'] == 'none'
    model_files = {p.relative_to(engine).as_posix():digest(p.read_bytes()) for p in files}
    observations = {'lean_version':lean_version, 'checked_model_sources':model_files,
        'coverage':{key:coverage[key] for key in ['schema','profile','certified_target','candidate_target','parser_boundary','resource_policy','external_compute_boundary']},
        'obligations':coverage['obligation'], 'extraction_toolchain_inventory':extraction,
        'proof_cap_control':{'test':cap_test,'test_only_cap':8,'production_cap_at_pin':100000,
            'observed_contract':'A lowered test-only cap refuses the whole certificate; after the override is removed, the same query returns a valid proof.'},
        'checks':{'lean_model_files_checked':True,'proof_cap_control_executed':True,'coverage_inventory_bound_to_engine':True,'no_whole_engine_certificate':True},
        'limits':['The checked Lean files prove their model statements under their stated definitions and hypotheses.',
            'This run does not execute production Rust extraction or issue a production correspondence receipt.',
            'The coverage inventory includes separate component proof work; it declares no completely certified engine target.',
            'Parsing, application formalization, external compute and deployment remain distinct trust boundaries.',
            'The separate Rust proof-cap regression uses a test-only cap of eight steps; it does not exhaust the 100000-step production cap.']}
    artifacts = {'model_sources_sha256':digest(json.dumps(model_files,sort_keys=True,separators=(',',':')).encode()),
        'coverage_inventory_sha256':digest(inventory_path.read_bytes()),'extraction_inventory_sha256':digest(extraction_path.read_bytes()),
        'engine_lock_sha256':digest((engine/'Cargo.lock').read_bytes()),
        'proof_cap_control_source_sha256':digest((engine/'nibli-reason/src/tests/resource_budgets.rs').read_bytes())}
    return observations, artifacts, [version,checked,cap_check]

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('lab',choices=LABS)
    parser.add_argument('--output',type=Path,required=True,help='new directory for reproducible input/output evidence')
    parser.add_argument('--engine-root',type=Path,help='clean checkout of the exact engine pin, required for WASI and formal models')
    args=parser.parse_args()
    if platform.system()!='Linux':raise SystemExit('This kit currently verifies Linux. Other platforms are unverified; no substitute result is fabricated.')
    args.output=args.output.resolve();args.output.mkdir(parents=True,exist_ok=False)
    files=inventory(); report={'schema':1,'kind':'nibli-local-lab-report','lab_id':args.lab,'lab_version':1,'engine_revision':ENGINE,'kit_sha256':digest(json.dumps(files,sort_keys=True,separators=(',',':')).encode()),'source_files':files,'platform':{'system':platform.system(),'release':platform.release(),'machine':platform.machine()},'tools':{'python':platform.python_version(),'rustc':tool(['rustc','--version']),'cargo':tool(['cargo','--version'])},'execution_claim':'learner-supplied; not authenticated by the companion','status':'failed'}
    try:
        if not report['tools']['rustc'].startswith('rustc 1.96.1 '):raise RuntimeError('Enter the pinned nix develop environment (Rust 1.96.1).')
        if args.lab in {'wasi-runtime','formal-models'}:
            if not args.engine_root:raise RuntimeError('--engine-root is required for this lab; clone the public engine at the documented pin.')
            experiment = wasi if args.lab == 'wasi-runtime' else formal_models
            observations,artifacts,runs=experiment(args.output,args.engine_root.resolve())
        else: observations,artifacts,runs=native(args.lab,args.output)
        report.update(status='passed',observations=observations,artifacts=artifacts,runs=runs)
    except (RuntimeError,AssertionError,subprocess.SubprocessError,OSError,ValueError) as error:
        report['failure']=str(error)
    report['logs']={p.name:{'sha256':digest(p.read_bytes()),'bytes':p.stat().st_size} for p in sorted(args.output.iterdir()) if p.suffix in {'.stdout','.stderr'}}
    (args.output/'report.json').write_text(json.dumps(report,indent=2)+'\n')
    print(f"{args.lab}: {report['status']} — {args.output/'report.json'}")
    if report['status']!='passed':print(report.get('failure','unknown failure'),file=sys.stderr);return 1
    return 0
if __name__=='__main__':sys.exit(main())
