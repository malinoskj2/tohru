from pathlib import Path
import subprocess
root=Path(subprocess.check_output(['git','rev-parse','--show-toplevel'],text=True).strip()); exe=root/'prototypes/pleamar/vendor/pleamar/target/release/pleamar'
shader=root/'prototypes/pleamar/ribbon.wgsl'
log=[]
for count,accepted in [(24,True),(25,False)]:
    path=root/'.tmp'/f'abi-{count}.plm'
    path.write_text('scene ABI {\n surface { size: 104, 42 }\n shader ribbon = file "'+str(shader)+'"\n shader ribbon { at: 0, 0; size: 104, 42; values: '+','.join(['0']*count)+' }\n}\n')
    r=subprocess.run([str(exe),'--check',str(path)],capture_output=True,text=True)
    log.append(f'count={count}, exit={r.returncode}, expected_accept={accepted}\n{r.stdout}{r.stderr}')
    assert (r.returncode==0)==accepted
path=root/'.tmp'/'abi-group-9.plm'
path.write_text('scene ABI {\n surface { size: 104, 42 }\n shader ribbon = file "'+str(shader)+'"\n group { shader: ribbon, '+','.join(['0']*9)+'; box { from: 0, 0; size: 10, 10; color: #ffffff } }\n}\n')
r=subprocess.run([str(exe),'--check',str(path)],capture_output=True,text=True)
log.append(f'group count=9, exit={r.returncode}, expected_reject=True\n{r.stdout}{r.stderr}')
assert r.returncode!=0
(root/'.tmp'/'abi-check.log').write_text('\n'.join(log))
