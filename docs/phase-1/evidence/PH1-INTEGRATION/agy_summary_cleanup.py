"""Remove TelosWeaver test conversations from Antigravity summary indexes (H0 D15).

Antigravity rebuilds conversation_summaries.db from jetbox_summaries_proto.pb, so rows
are dropped from the protobuf source, the SQLite index and cache/last_conversations.json
together. Only entries whose conversation files are gone and whose workspace is a telos-*
test folder or default-cli-project are removed. Dry run by default; --apply backs up first
(TELOS_BACKUP_DIR, default ~/.telos-agy-cleanup-backup).
"""
import os, sys, json, sqlite3, shutil, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__))); import agy_pbwire as pbwire
APPLY = "--apply" in sys.argv
g = os.path.join(os.path.expanduser("~"), ".gemini")
here = os.environ.get("TELOS_BACKUP_DIR") or os.path.join(os.path.expanduser("~"), ".telos-agy-cleanup-backup")
def has_files(cid):
    return any(os.path.exists(os.path.join(g, x, "brain", cid)) or any(os.path.exists(os.path.join(g, x, "conversations", cid + e)) for e in (".db", ".pb")) for x in ("antigravity", "antigravity-cli"))
def is_test(cid, text):
    return not has_files(cid) and (b"default-cli-project" in text or b"telos-" in text)
for d in ("antigravity", "antigravity-cli"):
    base = os.path.join(g, d)
    pb = os.path.join(base, "jetbox_summaries_proto.pb"); b = open(pb, "rb").read()
    c = sqlite3.connect(os.path.join(base, "conversation_summaries.db"), timeout=10)
    dbrows = {r[0]: (r[1] + " " + r[2]).encode() for r in c.execute("select conversation_id, project_id, workspace_uris from conversation_summaries")}
    keep, drop = [], set()
    for fn, wt, v, raw in pbwire.fields(b):
        cid = next(x for f, w, x, _ in pbwire.fields(v) if f == 1).decode()
        if is_test(cid, raw + dbrows.get(cid, b"")): drop.add(cid)
        else: keep.append(raw)
    drop |= {cid for cid, t in dbrows.items() if is_test(cid, t)}
    lc = os.path.join(base, "cache", "last_conversations.json"); lcj = json.load(open(lc, encoding="utf-8"))
    lc_drop = [k for k, v in lcj.items() if v in drop]
    total = sum(1 for _ in pbwire.fields(b))
    print(f"== {d}: pb entries {total}, drop {len(drop)} ids; db rows to drop {sum(1 for k in dbrows if k in drop)}/{len(dbrows)}; last_conversations drop {len(lc_drop)}/{len(lcj)}")
    if not APPLY:
        continue
    bk = os.path.join(here, "backup-" + time.strftime("%H%M%S"), d); os.makedirs(bk, exist_ok=True)
    for f in ("jetbox_summaries_proto.pb", "cache/last_conversations.json"):
        shutil.copy2(os.path.join(base, f), os.path.join(bk, f.replace("/", "_")))
    bkdb = sqlite3.connect(os.path.join(bk, "conversation_summaries.db")); c.backup(bkdb); bkdb.close()
    tmp = pb + ".telos-tmp"; open(tmp, "wb").write(b"".join(keep)); os.replace(tmp, pb)
    for k in lc_drop: del lcj[k]
    tmp = lc + ".telos-tmp"; json.dump(lcj, open(tmp, "w", encoding="utf-8")); os.replace(tmp, lc)
    tables = {r[0] for r in c.execute("select name from sqlite_master")}
    ids = sorted(drop); q = ",".join("?" * len(ids))
    with c:
        n = c.execute(f"delete from conversation_summaries where conversation_id in ({q})", ids).rowcount
        m = c.execute(f"delete from conversations_fts where conversation_id in ({q})", ids).rowcount if "conversations_fts" in tables else 0
    print(f"   applied: pb kept {len(keep)}, db deleted {n} (fts {m}), integrity {c.execute('PRAGMA integrity_check').fetchone()[0]}")
    c.close()
