#!/usr/bin/env python3
"""Independently verify Ordivon M6 transactional evidence."""
from __future__ import annotations
import json, math, sys
from pathlib import Path
from statistics import median
from typing import Any

REV = "ffaca2f1989573d8d085d5cbeaa96b6a91726b35"
JOURNEYS = ["readonly-audit","single-file-edit","multi-file-test","failure-repair-loop","rust-target-test","bounded-log-artifact","durable-recovery-cancel","idempotency-and-list"]
SHADOW = JOURNEYS[:4]
RUNTIME = [
 "m6_ambiguous_dispatch_is_lost_without_automatic_redispatch",
 "m6_cancel_intent_survives_runtime_reconstruction_and_cleans_cgroup",
 "m6_core_restart_recovers_running_attempt_and_terminal_result",
 "m6_corrupt_runner_result_is_orphaned_and_quarantined",
 "m6_fast_failures_never_race_into_lost",
 "m6_live_unit_without_launch_token_is_orphaned_and_holds_capacity",
 "m6_reconciler_rebuilds_bundle_after_admission_commit",
 "m6_transactional_runtime_executes_replays_and_releases_capacity",
]

def fail(msg: str) -> None: raise SystemExit(f"M6_EVIDENCE_FAIL: {msg}")
def load(path: str) -> dict[str, Any]:
 try: return json.loads(Path(path).read_text())
 except Exception as e: fail(f"cannot load {path}: {e}")
def req(actual: Any, expected: Any, label: str) -> None:
 if actual != expected: fail(f"{label}: expected {expected!r}, got {actual!r}")
def revision(d: dict[str, Any], label: str) -> None: req(d.get("sourceRevision"), REV, f"{label} revision")
def rounded_median(v: list[int]) -> int: return int(math.floor(median(v)+0.5))
def shadow_summary(samples: list[dict[str, Any]]) -> dict[str, Any]:
 return {"samples":len(samples),"succeeded":all(x["succeeded"] for x in samples),"elapsedMs":rounded_median([x["elapsedMs"] for x in samples]),"toolCalls":rounded_median([x["toolCalls"] for x in samples]),"contextBytes":rounded_median([x["contextBytes"] for x in samples]),"outputBytes":rounded_median([x["outputBytes"] for x in samples]),"httpRequests":rounded_median([x["httpRequests"] for x in samples]),"repairRounds":rounded_median([x["repairRounds"] for x in samples]),"fallbackCount":sum(x["fallbackCount"] for x in samples)}
def perf_summary(values: list[int]) -> dict[str, int]:
 s=sorted(values); n=len(s); return {"samples":n,"p50Us":s[n//2],"p95Us":s[((n*95+99)//100)-1],"maxUs":s[-1]}

def verify_wire(d: dict[str, Any]) -> None:
 req(d["phase"],"ORDIVON-M6-WIRE-CONTRACT","wire phase"); revision(d,"wire"); req(d["passed"],True,"wire pass"); req(d["concurrentReads"],64,"wire concurrency")
 if d["coreTraceRows"] < 73 or d["httpTraceRows"] < 87: fail("wire trace rows too small")
 budgets={"workspace.open":220,"workspace.read":360,"workspace.mutate":460,"workspace.exec":420,"task.list":420,"artifact.read":560,"workspace.diff":900}
 for item in d["observations"]:
  if item["name"] in budgets and item["bytes"] > budgets[item["name"]]: fail(f"wire budget {item['name']}")

def verify_transport(d: dict[str, Any]) -> None:
 revision(d,"transport"); req(d["results"],{"missingAuthorization":401,"invalidAuthorization":401,"disallowedOrigin":403,"disallowedHost":403,"oversizedBody":413},"transport results")

def verify_dogfood(d: dict[str, Any]) -> None:
 revision(d,"dogfood"); js=d["journeys"]; req([x["name"] for x in js],JOURNEYS,"dogfood journeys")
 summary={"journeyCount":len(js),"succeeded":all(x["succeeded"] for x in js),"totalToolCalls":sum(x["toolCalls"] for x in js),"totalContextBytes":sum(x["contextBytes"] for x in js),"totalOutputBytes":sum(x["outputBytes"] for x in js),"totalRepairRounds":sum(x["repairRounds"] for x in js),"fallbackCount":sum(x["fallbackCount"] for x in js),"recoveredAfterDisconnect":any(x.get("recoveredAfterDisconnect") is True for x in js),"cancellationClean":any(x.get("cancellationClean") is True for x in js),"idempotentReplay":any(x.get("idempotentReplay") is True for x in js)}
 req(d["summary"],summary,"dogfood summary"); req(summary["totalRepairRounds"],1,"repair rounds"); req(summary["fallbackCount"],0,"dogfood fallback")

def verify_concurrency(d: dict[str, Any]) -> None:
 revision(d,"concurrency"); levels=d["levels"]; req([x["level"] for x in levels],[2,4,8],"concurrency levels")
 released=0
 for x in levels:
  released += x["level"]; req(x["jobs"],x["level"],"job count"); req(x["uniqueJobs"],x["level"],"unique jobs"); req(x["activeAfterAdmission"],x["level"],"active admission"); req(x["activeAfterCompletion"],0,"active completion"); req(x["releasedAfterCompletion"],released,"released count"); req(x["overflowCode"],"CONCURRENCY_LIMIT","overflow")
 req(d["totals"]["finalRegistry"],{"jobs":14,"attempts":14,"active":0,"released":14},"final registry"); req(all(d["gates"].values()),True,"concurrency gates")

def verify_shadow(d: dict[str, Any]) -> None:
 revision(d,"shadow"); req(d["iterationsPerJourney"],3,"shadow iterations"); req(d["journeyKinds"],SHADOW,"shadow kinds"); req(d["alternatingOrder"],True,"shadow order")
 m5=d["rawSamples"]["m5"]; m6=d["rawSamples"]["m6"]; req(len(m5),12,"m5 samples"); req(len(m6),12,"m6 samples")
 a={x["pairId"]:x for x in m5}; b={x["pairId"]:x for x in m6}; req(set(a),set(b),"shadow pairs")
 for key in a: req(a[key]["semanticDigest"],b[key]["semanticDigest"],f"semantic {key}")
 sm5=shadow_summary(m5); sm6=shadow_summary(m6); req(d["summaries"]["overall"]["m5"],sm5,"m5 summary"); req(d["summaries"]["overall"]["m6"],sm6,"m6 summary")
 for kind in SHADOW:
  req(d["summaries"]["byKind"][kind]["m5"],shadow_summary([x for x in m5 if x["kind"]==kind]),f"m5 {kind}"); req(d["summaries"]["byKind"][kind]["m6"],shadow_summary([x for x in m6 if x["kind"]==kind]),f"m6 {kind}")
 gates={"completionNotWorse":sm5["succeeded"] and sm6["succeeded"],"semanticEquivalence":True,"repairRoundsNotWorse":sm6["repairRounds"]<=sm5["repairRounds"],"toolCallsNotWorse":sm6["toolCalls"]<=sm5["toolCalls"],"contextWithinTwentyFivePercent":sm6["contextBytes"]<=math.ceil(sm5["contextBytes"]*1.25),"elapsedWithinTwentyFivePercent":sm6["elapsedMs"]<=math.ceil(sm5["elapsedMs"]*1.25),"noFallback":sm6["fallbackCount"]==0}
 req(d["decision"]["gates"],gates,"shadow gates"); req(d["decision"]["transactionalDogfoodEligible"],all(gates.values()),"shadow decision")

def verify_performance(d: dict[str, Any]) -> None:
 revision(d,"performance"); expected={k:perf_summary(v) for k,v in d["rawSamplesUs"].items()}; req(d["summaries"],expected,"performance summaries")
 p=lambda k:expected[k]["p95Us"]; gates={"admissionP95AtMost50Ms":p("admission")<=50000,"replayP95AtMost20Ms":p("replay")<=20000,"statusP95AtMost20Ms":p("status")<=20000,"list100P95AtMost50Ms":p("list100")<=50000,"terminalP95AtMost50Ms":p("terminal")<=50000}; req(d["gates"],gates,"performance gates"); req(all(gates.values()),True,"performance pass")

def verify_model(d: dict[str, Any]) -> None:
 revision(d,"model"); req(len(d["steps"]),7,"model steps"); s=d["summary"]
 for key in ["completed","modelSelectedTools","modelReplannedFromObservations","serverGeneratedJobIdentity","registryProjectionVerified"]: req(s[key],True,f"model {key}")
 req(s["fallbackCount"],0,"model fallback"); req(s["productionFilesChanged"],False,"model production mutation")

def verify_runtime(d: dict[str, Any]) -> None:
 revision(d,"runtime"); req(d["tests"],RUNTIME,"runtime tests"); req(d["passed"],8,"runtime passed"); req(d["failed"],0,"runtime failed"); req(d["fastFailureIterations"],10,"fast failure iterations")

def main() -> None:
 if len(sys.argv)!=9: fail("usage: check_m6_evidence.py WIRE TRANSPORT DOGFOOD CONCURRENCY SHADOW PERFORMANCE MODEL RUNTIME")
 data=[load(x) for x in sys.argv[1:]]
 for fn,d in zip([verify_wire,verify_transport,verify_dogfood,verify_concurrency,verify_shadow,verify_performance,verify_model,verify_runtime],data): fn(d)
 print("M6_EVIDENCE_PASS")
if __name__=="__main__": main()
