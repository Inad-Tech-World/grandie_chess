#!/usr/bin/env python3
"""Run JavaScript in a debug build of Grandie Chess started with
GRANDIE_DEV_DRIVER=<folder> (see src-tauri/src/dev_driver.rs).

    scripts/drive.py <folder> 'return document.title'
    scripts/drive.py <folder> -f steps.js --timeout 120

The script is the body of an async function in the main window; what it
returns is printed as JSON. Exits 1 if the script threw or timed out.
"""
import argparse
import json
import os
import sys
import time
import uuid


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("folder")
    ap.add_argument("code", nargs="?")
    ap.add_argument("-f", "--file")
    ap.add_argument("--timeout", type=float, default=60)
    args = ap.parse_args()
    code = open(args.file).read() if args.file else args.code
    if not code:
        ap.error("give the code or -f FILE")
    name = f"{time.time():.3f}-{uuid.uuid4().hex[:6]}"
    inbox = os.path.join(args.folder, "in")
    out = os.path.join(args.folder, "out", f"{name}.json")
    os.makedirs(inbox, exist_ok=True)
    tmp = os.path.join(inbox, f"{name}.part")
    with open(tmp, "w") as f:
        f.write(code)
    os.rename(tmp, os.path.join(inbox, f"{name}.js"))
    deadline = time.time() + args.timeout
    while time.time() < deadline:
        if os.path.exists(out):
            with open(out) as f:
                answer = json.load(f)
            os.remove(out)
            if answer.get("ok"):
                print(json.dumps(answer.get("result"), indent=1, ensure_ascii=False))
                return 0
            print(answer.get("error"), file=sys.stderr)
            return 1
        time.sleep(0.1)
    print(f"no answer within {args.timeout:.0f}s", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
