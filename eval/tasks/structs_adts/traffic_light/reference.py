from enum import Enum
class Light(Enum):
    Red = 1
    Green = 2
    Yellow = 3
dur = {Light.Red: 30, Light.Green: 25, Light.Yellow: 5}
nxt = {Light.Red: Light.Green, Light.Green: Light.Yellow, Light.Yellow: Light.Red}
t, s = 0, Light.Red
while t < 100:
    print(f"t={t} {s.name}")
    t += dur[s]
    s = nxt[s]
t, s = 0, Light.Red
while t + dur[s] <= 100:
    t += dur[s]
    s = nxt[s]
print(f"at 100: {s.name}")
