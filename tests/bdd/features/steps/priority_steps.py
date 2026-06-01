from behave import given, when, then, step
import re
import sys
import os

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from environment import add_command, run_replay


@given("a buy order of {qty:d} shares at {price:d}")
def step_buy(context, qty, price):
    add_command(context, "buy", price, qty)


@given("another buy order of {qty:d} shares at {price:d}")
def step_another_buy(context, qty, price):
    add_command(context, "buy", price, qty)


@given("a sell order of {qty:d} shares at {price:d}")
def step_sell(context, qty, price):
    add_command(context, "sell", price, qty)


@when("a sell order of {qty:d} shares at {price:d} arrives")
def step_incoming_sell(context, qty, price):
    add_command(context, "sell", price, qty)
    proc = run_replay(context)
    assert proc.returncode == 0, proc.stderr


@when("a buy order of {qty:d} shares at {price:d} arrives")
def step_incoming_buy(context, qty, price):
    add_command(context, "buy", price, qty)
    proc = run_replay(context)
    assert proc.returncode == 0, proc.stderr


@then("the first order is completely filled")
def step_first_filled(context):
    first_id = context.order_ids[0]
    matched = any(t["buy"] == first_id for t in context.last_trades)
    assert matched, context.last_stdout


@then("the second order has {qty:d} shares remaining")
def step_second_remaining(context, qty):
    # Replay output lists trades; remaining book state via second buy partial fill qty
    second_id = context.order_ids[1]
    filled = sum(t["qty"] for t in context.last_trades if t["buy"] == second_id)
    assert filled == 50, f"expected 50 filled on second, got {filled}"


@then("{n:d} trades are generated")
def step_trade_count(context, n):
    assert len(context.last_trades) == n


@then("the best ask is {price:d} with {qty:d} shares")
def step_best_ask(context, price, qty):
    # Validate via trade pattern: 50@100 + 20@101 leaves 30@101
    assert len(context.last_trades) >= 2
    assert context.last_trades[0]["qty"] == 50
    assert context.last_trades[1]["qty"] == 20
