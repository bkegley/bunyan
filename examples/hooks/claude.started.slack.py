#!/usr/bin/env python3
# Post to Slack whenever a Claude session starts. Reads the JSON payload from
# stdin instead of env vars — there's just as much info there and parsing is
# nicer in Python.
import json
import os
import sys
import urllib.request

event = json.load(sys.stdin)
webhook = os.environ.get("SLACK_WEBHOOK_URL")
if not webhook:
    # Silently skip if not configured — bunyan logs this as a debug.
    sys.exit(0)

msg = (
    f"Claude session started: {event['repo']['name']}/"
    f"{event['workspace']['name']}"
)
req = urllib.request.Request(
    webhook,
    data=json.dumps({"text": msg}).encode(),
    headers={"Content-Type": "application/json"},
)
urllib.request.urlopen(req, timeout=5)
