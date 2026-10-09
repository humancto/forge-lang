-- Port of benchmarks/vm/spectral_norm.fg
local function a(i, j)
  return 1.0 / ((i + j) * (i + j + 1) / 2 + i + 1)
end

local function mul_av(v, n)
  local out = {}
  for i = 0, n - 1 do
    local s = 0.0
    for j = 0, n - 1 do
      s = s + a(i, j) * v[j + 1]
    end
    out[#out + 1] = s
  end
  return out
end

local function mul_atv(v, n)
  local out = {}
  for i = 0, n - 1 do
    local s = 0.0
    for j = 0, n - 1 do
      s = s + a(j, i) * v[j + 1]
    end
    out[#out + 1] = s
  end
  return out
end

local function spectral_norm(n)
  local u = {}
  for i = 1, n do
    u[#u + 1] = 1.0
  end
  local v = {}
  for k = 1, 10 do
    v = mul_atv(mul_av(u, n), n)
    u = mul_atv(mul_av(v, n), n)
  end
  local vbv = 0.0
  local vv = 0.0
  for i = 1, n do
    vbv = vbv + u[i] * v[i]
    vv = vv + v[i] * v[i]
  end
  return math.sqrt(vbv / vv)
end
print(string.format("%d", math.floor(spectral_norm(100) * 1000000000 + 0.5)))
