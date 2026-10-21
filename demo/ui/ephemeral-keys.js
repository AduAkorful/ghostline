// SPDX-License-Identifier: MIT

function freshSecret(randomBytes, scalarFromWide) {
  let scalar = 0n;
  while (scalar === 0n) scalar = scalarFromWide(randomBytes(64));
  const secret = new Uint8Array(32);
  for (let index = 0; index < secret.length; index += 1) {
    secret[index] = Number(scalar & 0xffn);
    scalar >>= 8n;
  }
  return secret;
}

/** Create independent payer and settlement-output keypairs for one browser simulation. */
export function createEphemeralChannelKeys(randomBytes, scalarFromWide, publicKeyFromSecret) {
  const payerSecret = freshSecret(randomBytes, scalarFromWide);
  const serviceOutputSecret = freshSecret(randomBytes, scalarFromWide);
  return {
    payerSecret,
    payerPublic: publicKeyFromSecret(payerSecret),
    serviceOutputSecret,
    serviceOutputPublic: publicKeyFromSecret(serviceOutputSecret),
  };
}
