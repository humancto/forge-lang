for i in range(1, 36):
    s = ""
    if i % 3 == 0: s += "Fizz"
    if i % 5 == 0: s += "Buzz"
    if i % 7 == 0: s += "Bazz"
    print(s or i)
