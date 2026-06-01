import os
import subprocess
import json
import tempfile


def _bin():
    return os.environ.get(
        "LOB_ENGINE_BIN",
        os.path.join(
            "target",
            "release",
            "lob-engine.exe" if os.name == "nt" else "lob-engine",
        ),
    )


def before_scenario(context, scenario):
    context.commands = []
    context.order_ids = []
    context.last_trades = []
    context.tmp = tempfile.TemporaryDirectory()


def after_scenario(context, scenario):
    if hasattr(context, "tmp"):
        context.tmp.cleanup()


def add_command(context, side, price, qty):
    import uuid

    oid = str(uuid.uuid4())
    context.order_ids.append(oid)
    context.commands.append(
        {
            "action": "add-limit",
            "order_id": oid,
            "side": side,
            "price": price,
            "quantity": qty,
            "timestamp": len(context.commands) + 1,
        }
    )
    return oid


def run_replay(context):
    path = os.path.join(context.tmp.name, "commands.json")
    with open(path, "w", encoding="utf-8") as f:
        json.dump(context.commands, f)
    proc = subprocess.run(
        [_bin(), "replay", path, "--commands"],
        capture_output=True,
        text=True,
        env={**os.environ, "LOB_SKIP_DB": "1"},
    )
    context.last_stdout = proc.stdout
    context.last_stderr = proc.stderr
    context.last_returncode = proc.returncode
    trades = []
    for line in proc.stdout.splitlines():
        if line.startswith("TRADE "):
            parts = line.split()
            trades.append(
                {
                    "qty": int(parts[5].split("=", 1)[1]),
                    "buy": parts[2].split("=", 1)[1],
                    "sell": parts[3].split("=", 1)[1],
                }
            )
    context.last_trades = trades
    return proc
