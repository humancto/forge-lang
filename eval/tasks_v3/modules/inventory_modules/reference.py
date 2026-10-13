import re
import sys

# Produces the expected output of the Forge modules driven by files/main.fg.
stock = {}
ops = 0


def dollars(c):
    return "{}.{:02d}".format(c // 100, c % 100)


def parse_item(text):
    f = text.split("|")
    if len(f) != 4:
        return None, "expected 4 fields"
    sku = f[0].strip()
    if not re.fullmatch(r"[A-Z]{2}-[0-9]{3}", sku):
        return None, "bad sku " + sku
    name = f[1].strip()
    if not name:
        return None, "missing name"
    qty = f[2].strip()
    if not re.fullmatch(r"[0-9]+", qty) or int(qty) == 0:
        return None, "bad quantity " + qty
    price = f[3].strip()
    m = re.fullmatch(r"([0-9]+)(?:\.([0-9]{1,2}))?", price)
    if not m:
        return None, "bad price " + price
    cents = int(m.group(1)) * 100 + (int(m.group(2).ljust(2, "0")) if m.group(2) else 0)
    return {"sku": sku, "name": name, "qty": int(qty), "price": cents}, None


def label(i):
    return "{} ({})".format(i["sku"], i["name"])


for line in sys.stdin.read().split("\n"):
    cmd = line.strip()
    if not cmd:
        continue
    if cmd.startswith("add "):
        item, err = parse_item(cmd[4:])
        if err:
            print("error: " + err)
            continue
        qty = item["qty"] + (stock[item["sku"]]["qty"] if item["sku"] in stock else 0)
        stock[item["sku"]] = dict(item, qty=qty)
        ops += 1
        print("added {}: {} in stock".format(label(item), qty))
    elif cmd.startswith("take "):
        _, sku, n = cmd.split(" ")
        n = int(n)
        if sku not in stock:
            print("error: unknown sku " + sku)
        elif n > stock[sku]["qty"]:
            print("error: only {} {} in stock".format(stock[sku]["qty"], sku))
        else:
            left = stock[sku]["qty"] - n
            if left == 0:
                del stock[sku]
            else:
                stock[sku]["qty"] = left
            ops += 1
            print("took {} {}: {} left".format(n, sku, left))
    elif cmd == "report":
        all_items = [stock[k] for k in sorted(stock)]
        units = sum(i["qty"] for i in all_items)
        total = sum(i["qty"] * i["price"] for i in all_items)
        print("report: {} skus, {} units, value {}".format(len(all_items), units, dollars(total)))
        for i in sorted(all_items, key=lambda i: (-i["qty"] * i["price"], i["sku"])):
            print("  {} x{} = {}".format(label(i), i["qty"], dollars(i["qty"] * i["price"])))
    else:
        print("error: unknown command")
print("operations: {}".format(ops))
print("skus: " + ",".join(sorted(stock)))
