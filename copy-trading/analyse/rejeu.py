# Rejeu de ses trades (data-api, 30 j) avec 1000 $ : règles actuelles vs proposées, exécution à son prix (maker idéal)
# ou à son prix + 1 tick avec frais preneur. Limites : pas de file d'attente ni de sélection adverse (maker = borne haute),
# valeur de son portefeuille supposée constante (HV), marchés ouverts valorisés au dernier prix CLOB.
# Données : python fetch.py <dir>/his30.json <début unix> ; python res.py <dir>/his30.json <dir>/res30.json ;
# puis python -X utf8 rejeu.py <dir>.
import json,sys,collections
S=sys.argv[1]; A=json.load(open(S+"/his30.json")); R=json.load(open(S+"/res30.json"))
T=sorted([x for x in A if x["type"]=="TRADE"],key=lambda x:(x["timestamp"],x["transactionHash"]))
HV=5e6; CAP=1000.0
SPORT=("nba-","nfl-","efl-","epl","mlb-","nhl-","cfb-","ncaa","ucl-","lal-","bun-","sea-","fl1-","atp-","wta-","ufc-","cbb-")
def cat(x): return "sport" if x["eventSlug"].startswith(SPORT) else "autre"
def fee(x,p,sh): return sh*(0.05 if cat(x)=="sport" else 0.04)*p*(1-p)
def final(a,c):
    m=R.get(c,{}); t=m.get("tokens",{}).get(a,{})
    if m.get("closed") and t.get("win") is not None: return 1.0 if t["win"] else 0.0
    return t.get("price") or 0.0
cid={x["asset"]:x["conditionId"] for x in T}; EV={x["asset"]:x["eventSlug"] for x in T}
LAST={}
for x in T: LAST[x["conditionId"]]=x["timestamp"]
TIERS=[(0.00001,0.03),(0.001,0.05),(0.01,0.10)]
def tier(p): return max([m for s,m in TIERS if p>=s],default=0)
def run(rule,exe,protect=False,L=1.0,ev_cap=0.15,max_order=20.0,thr=0,t0=0):
    cash=CAP; pos=collections.defaultdict(float); cost=collections.defaultdict(float)
    his=collections.defaultdict(float); hcum=collections.defaultdict(float); hcost=collections.defaultdict(float)
    mine_in=collections.defaultdict(float); ev_in=collections.defaultdict(float); n=0; skipped=0; fees=0
    pend=sorted((LAST[c]+4*3600,c) for c in LAST if R.get(c,{}).get("closed")); pi=0
    for x in T:
        while pi<len(pend) and pend[pi][0]<=x["timestamp"]:
            cc=pend[pi][1]; pi+=1
            for aa in list(pos):
                if cid[aa]==cc and pos[aa]>0: cash+=pos[aa]*final(aa,cc); ev_in[EV[aa]]-=mine_in[aa]; pos[aa]=0; cost[aa]=0
        if x["timestamp"]<t0: continue
        a=x["asset"]; c=x["conditionId"]; p=x["price"]; m=R.get(c,{}); tick=float(m.get("tick") or 0.01); mn=float(m.get("min") or 5)
        buy=x["side"]=="BUY"; hb=his[a]
        if buy: his[a]+=x["size"]; hcum[a]+=x["usdcSize"]; hcost[a]+=x["usdcSize"]
        else:
            if hb>0: hcost[a]*= max(0,1-x["size"]/hb)
            his[a]=max(0,hb-x["size"])
        if buy:
            px=p if exe=="maker" else min(0.99,round(p+tick,4))
            if rule=="actuel_taker":
                if mine_in[a]>0: continue
                usd=min(max(tier(x["usdcSize"]/HV)*(cash+sum(pos[k]*0.5 for k in pos)), mn*px),cash,max_order)
            elif rule=="actuel_maker":
                tgt=min(max(tier(hcum[a]/HV)*CAP, mn*px), max_order) if tier(hcum[a]/HV)>0 else 0
                usd=min(tgt-mine_in[a],cash)
            elif hcum[a]<thr: continue
            else: # proportionnel : ma part suit son engagement net, L x, plafond par événement
                tgt=L*(hcum[a]/HV)*CAP
                usd=min(tgt-mine_in[a], ev_cap*CAP-ev_in[x["eventSlug"]], cash)
            if usd<mn*px: skipped+=1; continue
            sh=usd/px; f=fee(x,px,sh) if exe=="taker" else 0
            cash-=usd+f; fees+=f; pos[a]+=sh; cost[a]+=usd; mine_in[a]+=usd; ev_in[x["eventSlug"]]+=usd; n+=1
        else:
            if pos[a]<=0 or hb<=0: continue
            frac=min(1,x["size"]/hb); sh=pos[a]*frac
            if sh*p<1 and frac<1: continue
            px=p if exe=="maker" else max(0.01,round(p-tick,4))
            avg=cost[a]/pos[a]; his_avg=(hcost[a]/his[a]) if his[a]>0 else p
            if protect and px<avg and p>=his_avg*0.98:  # lui sort en gain, nous en perte : on garde (vente posée au point mort)
                continue
            f=fee(x,px,sh) if exe=="taker" else 0
            cash+=sh*px-f; fees+=f; cost[a]-=avg*sh; pos[a]-=sh; n+=1
    val=cash+sum(sh*final(a,cid[a]) for a,sh in pos.items() if sh>0)
    return round(val-CAP,1), n, skipped, round(fees,1)
END=max(x["timestamp"] for x in T)
for days in (30,14,7,3.5):
    t0=END-days*86400
    print(f"--- {days} derniers jours")
    for lab,kw in [("actuel taker",dict(rule="actuel_taker",exe="taker")),("actuel maker",dict(rule="actuel_maker",exe="maker")),
                   ("prop L1 maker",dict(rule="prop",exe="maker",protect=True)),("prop L1 taker",dict(rule="prop",exe="taker",protect=True)),
                   ("prop L1 10k+ maker",dict(rule="prop",exe="maker",protect=True,thr=1e4)),("prop L1 10k+ taker",dict(rule="prop",exe="taker",protect=True,thr=1e4)),
                   ("prop L2 10k+ maker",dict(rule="prop",exe="maker",protect=True,thr=1e4,L=2)),("prop L2 10k+ taker",dict(rule="prop",exe="taker",protect=True,thr=1e4,L=2))]:
        print(f"  {lab:20}",run(t0=t0,**kw))
# lui sur la même période, même échelle
hv=collections.defaultdict(float); hc=0
for x in T:
    if x['timestamp']<END-3.5*86400: continue
    if x["side"]=="BUY": hv[x["asset"]]+=x["size"]; hc-=x["usdcSize"]
    else: hv[x["asset"]]-=x["size"]; hc+=x["usdcSize"]
his_pnl=hc+sum(s*final(a,cid[a]) for a,s in hv.items() if s>0)
print("lui 3,5 j (trades de la période seuls) :",round(his_pnl),"$ ; ramené à 1000 $ de 5 M$ :",round(his_pnl/HV*CAP,1))
