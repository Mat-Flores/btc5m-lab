import json,urllib.request,sys,time
W="0xf0318c32136c2db7fec88b84869aee6a1106c80c"; out=sys.argv[1]
def get(u):
    for _ in range(3):
        try: return json.load(urllib.request.urlopen(urllib.request.Request(u,headers={"User-Agent":"x"}),timeout=20))
        except Exception as e: err=e; time.sleep(1)
    raise err
rows=[]; end=None; start=int(sys.argv[2])
while True:
    u=f"https://data-api.polymarket.com/activity?user={W}&limit=500&start={start}"+(f"&end={end}" if end else "")
    r=get(u)
    if not r: break
    rows+=r; mn=min(x["timestamp"] for x in r)
    print(len(rows),mn,file=sys.stderr)
    if len(r)<500 or end==mn: break
    end=mn
seen=set(); uniq=[]
for x in rows:
    k=(x["transactionHash"],x.get("asset"),x["type"],x["size"],x["timestamp"])
    if k not in seen: seen.add(k); uniq.append(x)
json.dump(uniq,open(out,"w"))
print(len(uniq),file=sys.stderr)
