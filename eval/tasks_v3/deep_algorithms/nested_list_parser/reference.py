import sys

sys.setrecursionlimit(100000)


class ParseError(Exception):
    def __init__(self, pos, what):
        self.pos = pos
        self.what = what


class Parser:
    def __init__(self, text):
        self.s = text
        self.i = 0
        self.lists = 0
        self.ints = 0
        self.depth = 0
        self.weighted = 0

    def skip(self):
        while self.i < len(self.s) and self.s[self.i] == " ":
            self.i += 1

    def fail(self):
        what = "end of line" if self.i >= len(self.s) else "'" + self.s[self.i] + "'"
        raise ParseError(self.i + 1, what)

    def peek(self):
        return self.s[self.i] if self.i < len(self.s) else ""

    def value(self, depth):
        self.skip()
        c = self.peek()
        if c == "[":
            return self.list(depth + 1)
        if c == "-" or c.isdigit():
            start = self.i
            if c == "-":
                self.i += 1
            if not self.peek().isdigit():
                self.fail()
            while self.peek().isdigit():
                self.i += 1
            n = int(self.s[start:self.i])
            self.ints += 1
            self.weighted += n * depth
            return
        self.fail()

    def list(self, depth):
        self.i += 1
        self.lists += 1
        self.depth = max(self.depth, depth)
        self.skip()
        if self.peek() == "]":
            self.i += 1
            return
        while True:
            self.value(depth)
            self.skip()
            c = self.peek()
            if c == ",":
                self.i += 1
            elif c == "]":
                self.i += 1
                return
            else:
                self.fail()


for line in sys.stdin.read().split("\n"):
    if line.strip() == "":
        continue
    p = Parser(line)
    try:
        p.value(0)
        p.skip()
        if p.i < len(line):
            p.fail()
        print("depth={} lists={} ints={} weighted={}".format(p.depth, p.lists, p.ints, p.weighted))
    except ParseError as e:
        print("error at col {}: unexpected {}".format(e.pos, e.what))
