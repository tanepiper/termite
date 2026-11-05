"""Copy every pinned weight file into the bucket the Space mounts at /models, so a
run never downloads. The copy happens on the Hub and is quick. Run it after adding a
model to a list. Needs HF_TOKEN.

Usage: uvx --with huggingface_hub python sandbox/zerogpu/mirror.py
"""
import json,sys,fnmatch
from huggingface_hub import HfApi
api=HfApi()
B='tanepiper/termite-models'
have={f.path for f in api.list_bucket_tree(B,recursive=True) if getattr(f,'type','file')=='file'}
rows=[]
for p in ('eval/hosted-decision-models.jsonl','eval/zerogpu-core.jsonl'):
    rows+=[json.loads(l) for l in open(p)]
seen=set()
for r in rows:
    key=(r['repo'],r['revision'],r['file'])
    if key in seen: continue
    seen.add(key)
    files=[r['file']]
    if '-00001-of-' in r['file']:
        pat=r['file'].replace('-00001-of-','-*-of-')
        files=[f for f in api.list_repo_files(r['repo'],revision=r['revision']) if fnmatch.fnmatch(f,pat)]
    for f in files:
        dest=f"{r['repo']}/{r['revision']}/{f}"
        if dest in have: continue
        try:
            api.copy_files(f"hf://models/{r['repo']}@{r['revision']}/{f}",f"hf://buckets/{B}/{dest}")
            print('copied',dest,flush=True)
        except Exception as e:
            print('FAILED',dest,str(e)[:160],flush=True)
print('MIRROR-DONE',len(seen))
