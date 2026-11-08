"""Actual patched native parser contract, read-only --check; no compositor/IPC."""
import argparse
import os
from pathlib import Path
import subprocess
import tempfile

parser=argparse.ArgumentParser()
parser.add_argument('--runtime',type=Path,required=True)
args=parser.parse_args()
root=Path(__file__).resolve().parents[2]
(root/'.tmp').mkdir(exist_ok=True)
cases=[
    ('default','',True,''),
    ('named','layer_namespace: "tohru-launcher-backdrop";',True,''),
    ('maximum','layer_namespace: "'+'a'*128+'";',True,''),
    ('empty','layer_namespace: "";',False,'1 to 128'),
    ('long','layer_namespace: "'+'a'*129+'";',False,'1 to 128'),
    ('space','layer_namespace: "bad namespace";',False,'1 to 128'),
    ('unicode','layer_namespace: "日本";',False,'1 to 128'),
    ('window','kind: window; layer_namespace: "private";',False,'only to layer'),
    ('lock','kind: lock; open: false; layer_namespace: "private";',False,'only to layer'),
]
with tempfile.TemporaryDirectory(prefix='b-namespace-check-',dir=root/'.tmp') as directory:
    for name,properties,ok,message in cases:
        path=Path(directory)/(name+'.plm')
        path.write_text('scene Namespace { surface { size: 300, 200; '+properties+' } }\n')
        result=subprocess.run([str(args.runtime.resolve()),'--check',str(path)],text=True,capture_output=True,timeout=15,check=False)
        assert (result.returncode==0)==ok,(name,result.stdout,result.stderr)
        assert message in result.stdout+result.stderr,(name,result.stdout,result.stderr)
print('Actual native namespace parser default/valid/bounds/nonlayer matrix passed')
