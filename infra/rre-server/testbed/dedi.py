#!/usr/bin/env python3
"""Minimal RRE dedicated-manager API client for the local testbed.

The manager's HttpListener only answers requests with Host: localhost:8088
(the production nginx sets the same header). Usage:

  python3 dedi.py list
  python3 dedi.py create --count 5 --prefix mrgg-spike
  python3 dedi.py start-all            # start every stopped server (quota 5)
  python3 dedi.py stop-all
  python3 dedi.py delete-all           # recovery after a manager restart (D-042)
  python3 dedi.py rule ID TARGET       # POST /result/rule/{id}
"""
import argparse
import json
import time
import urllib.request
from pathlib import Path

BASE = "http://127.0.0.1:8088"
TEMPLATE = Path(__file__).resolve().parents[3] / ".product/research/rre-dedi-default-gamesetting.json"


def call(path, method="GET", body=None, timeout=30):
    headers = {"Host": "localhost:8088", "Accept": "application/json"}
    data = None
    if body is not None:
        data = json.dumps(body).encode()
        headers["Content-Type"] = "application/json"
    req = urllib.request.Request(BASE + path, data=data, headers=headers, method=method)
    with urllib.request.urlopen(req, timeout=timeout) as r:
        raw = r.read().decode()
    try:
        return json.loads(raw)
    except ValueError:
        return raw


def state():
    rows = call("/dedi")
    return [] if isinstance(rows, dict) else rows


def summary(rows=None):
    out = []
    for row in rows if rows is not None else state():
        ps = row.get("ProcessState") or {}
        gs = row.get("GameSetting") or {}
        out.append({
            "id": gs.get("Id"),
            "name": gs.get("Name"),
            "running": ps.get("Running"),
            "port": ps.get("Port"),
            "time_port": ps.get("TimePort"),
            "players": ps.get("PlayersOnServer"),
            "session": ps.get("CurrentSession"),
            "time_left_ms": ps.get("TimeLeft"),
        })
    return out


def create(count, prefix):
    base = json.loads(TEMPLATE.read_text())["GameSetting"]
    created = []
    for i in range(1, count + 1):
        gs = json.loads(json.dumps(base))
        gs.update({"Id": 0, "Name": f"{prefix}-{i}", "Password": "", "MaxNumOfPlayers": 8})
        res = call("/dedi/", "POST", {"GameSettings": gs})
        created.append(res.get("ProcessId") if isinstance(res, dict) else None)
        time.sleep(0.5)
    return created


def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="cmd", required=True)
    sub.add_parser("list")
    p = sub.add_parser("create")
    p.add_argument("--count", type=int, default=1)
    p.add_argument("--prefix", default="mrgg-spike")
    sub.add_parser("start-all")
    sub.add_parser("stop-all")
    sub.add_parser("delete-all")
    p = sub.add_parser("rule")
    p.add_argument("id", type=int)
    p.add_argument("target")
    args = parser.parse_args()

    if args.cmd == "list":
        print(json.dumps(summary(), indent=2))
    elif args.cmd == "create":
        print(json.dumps(create(args.count, args.prefix)))
    elif args.cmd == "start-all":
        for row in state():
            if not (row.get("ProcessState") or {}).get("Running"):
                pid = (row.get("GameSetting") or {}).get("Id")
                print(pid, call("/dedi/start", "POST", {"ProcessId": pid}))
                time.sleep(4)
        print(json.dumps(summary(), indent=2))
    elif args.cmd == "stop-all":
        for row in state():
            if (row.get("ProcessState") or {}).get("Running"):
                pid = (row.get("GameSetting") or {}).get("Id")
                print(pid, call("/dedi/stop", "POST", {"ProcessId": pid}))
    elif args.cmd == "delete-all":
        for row in state():
            pid = (row.get("GameSetting") or {}).get("Id")
            print(pid, call(f"/dedi/{pid}", "DELETE"))
    elif args.cmd == "rule":
        print(call(f"/result/rule/{args.id}", "POST",
                   {"ProcessId": args.id, "UploadTarget": args.target}))


if __name__ == "__main__":
    main()
