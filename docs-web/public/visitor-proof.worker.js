self.onmessage = async ({ data }) => {
  const encoder = new TextEncoder();

  const hasLeadingZeroBits = (digest, difficulty) => {
    let remaining = difficulty;
    for (const byte of new Uint8Array(digest)) {
      if (remaining <= 0) return true;
      if (byte === 0) {
        remaining -= 8;
        continue;
      }
      for (let mask = 128; mask > 0 && remaining > 0; mask >>= 1) {
        if ((byte & mask) !== 0) return false;
        remaining -= 1;
      }
      return remaining <= 0;
    }
    return remaining <= 0;
  };

  try {
    for (let nonce = 0; nonce <= 20_000_000; nonce += 1) {
      const digest = await crypto.subtle.digest("SHA-256", encoder.encode(`${data.challenge}:${nonce}`));
      if (hasLeadingZeroBits(digest, data.difficulty)) {
        self.postMessage({ nonce });
        return;
      }
    }
    self.postMessage({ error: "Proof nonce limit reached" });
  } catch {
    self.postMessage({ error: "Proof worker failed" });
  }
};
