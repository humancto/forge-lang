class Account:
    def __init__(self, owner, balance):
        self.owner, self.balance, self.ok = owner, balance, 0
    def apply(self, op, amt):
        if op == "withdraw" and amt > self.balance:
            return False
        self.balance += amt if op == "deposit" else -amt
        self.ok += 1
        return True
a = Account("Ada", 10000)
for op, amt in [("deposit", 2500), ("withdraw", 4000), ("withdraw", 9000), ("deposit", 125),
                ("withdraw", 8625), ("withdraw", 1)]:
    if a.apply(op, amt):
        print(f"{op} {amt}: ok, balance {a.balance}")
    else:
        print(f"{op} {amt}: rejected (insufficient funds), balance {a.balance}")
print(f"{a.owner}: {a.balance} cents after {a.ok} successful operations")
