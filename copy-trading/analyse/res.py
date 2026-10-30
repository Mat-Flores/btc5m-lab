import json,sys,urllib.request
A=json.load(open(sys.argv[1])); out={}
for c in sorted({x["conditionId"] for x in A if x["type"]=="TRADE"}):
    try:
        m=json.load(urllib.request.urlopen(urllib.request.Request(f"https://clob.polymarket.com/markets/{c}",headers={"User-Agent":"x"}),timeout=20))
        out[c]={"closed":m.get("closed"),"q":m.get("question"),"tokens":{t["token_id"]:{"win":t.get("winner"),"price":t.get("price")} for t in m.get("tokens",[])},"tags":m.get("tags"),"min":m.get("minimum_order_size"),"tick":m.get("minimum_tick_size")}
    except Exception as e: out[c]={"err":str(e)}
json.dump(out,open(sys.argv[2],"w")); print(len(out), sum(1 for v in out.values() if v.get("closed")))
