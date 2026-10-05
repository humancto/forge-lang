// Share links: the program lives in the URL fragment (never sent to a
// server), deflate-compressed and base64url-encoded.
//
//   #code=<base64url(deflate-raw(utf8 source))>[&engine=vm|interp]
//   #src=<base64url(utf8 source)>   (written when CompressionStream is
//                                    unavailable; always readable)
//   #tour=<lesson number, 1-based>

const toBase64Url = (bytes) => {
  let bin = "";
  for (let i = 0; i < bytes.length; i += 0x8000) {
    bin += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  }
  return btoa(bin).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
};

const fromBase64Url = (text) => {
  const b64 = text.replace(/-/g, "+").replace(/_/g, "/");
  const bin = atob(b64 + "=".repeat((4 - (b64.length % 4)) % 4));
  return Uint8Array.from(bin, (c) => c.charCodeAt(0));
};

async function pipe(bytes, stream) {
  const out = new Response(new Blob([bytes]).stream().pipeThrough(stream));
  return new Uint8Array(await out.arrayBuffer());
}

/** The fragment (without '#') that encodes `source` (and an engine choice). */
export async function encodeShare(source, engine) {
  const bytes = new TextEncoder().encode(source);
  let fragment;
  if (typeof CompressionStream === "function") {
    fragment = "code=" + toBase64Url(await pipe(bytes, new CompressionStream("deflate-raw")));
  } else {
    fragment = "src=" + toBase64Url(bytes);
  }
  if (engine && engine !== "auto") fragment += "&engine=" + engine;
  return fragment;
}

/**
 * Parse a location fragment. Returns `{ source?, engine?, tour? }`; fields
 * are absent when not present or not decodable.
 */
export async function decodeShare(hash) {
  const params = new URLSearchParams(hash.replace(/^#/, ""));
  const result = {};
  const engine = params.get("engine");
  if (engine === "vm" || engine === "interp" || engine === "auto") result.engine = engine;
  const tour = Number.parseInt(params.get("tour") || "", 10);
  if (Number.isFinite(tour) && tour > 0) result.tour = tour;
  try {
    if (params.has("code") && typeof DecompressionStream === "function") {
      const bytes = await pipe(fromBase64Url(params.get("code")), new DecompressionStream("deflate-raw"));
      result.source = new TextDecoder().decode(bytes);
    } else if (params.has("src")) {
      result.source = new TextDecoder().decode(fromBase64Url(params.get("src")));
    }
  } catch (_) {
    // A truncated or edited link: ignore the code, keep the rest.
  }
  return result;
}
