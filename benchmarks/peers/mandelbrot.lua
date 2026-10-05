-- Port of benchmarks/vm/mandelbrot.fg
local function mandelbrot(size, max_iter)
  local inside = 0
  for py = 0, size - 1 do
    local ci = 2.0 * py / size - 1.0
    for px = 0, size - 1 do
      local cr = 2.0 * px / size - 1.5
      local zr = 0.0
      local zi = 0.0
      local i = 0
      while i < max_iter do
        local tr = zr * zr - zi * zi + cr
        zi = 2.0 * zr * zi + ci
        zr = tr
        if zr * zr + zi * zi > 4.0 then
          break
        end
        i = i + 1
      end
      if i == max_iter then
        inside = inside + 1
      end
    end
  end
  return inside
end
print(mandelbrot(200, 50))
