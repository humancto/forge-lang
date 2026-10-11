cents = 100000
for year in range(1, 7):
    interest = (cents * 35 + 500) // 1000
    cents += interest
    print(f"year {year}: {cents // 100}.{cents % 100:02d}")
