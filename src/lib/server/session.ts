import { createHmac, timingSafeEqual } from 'node:crypto';

export const SESSION_AGE = 7 * 24 * 60 * 60;
export const isSnowflake = (id: string) => /^[1-9][0-9]{0,18}$/.test(id) && BigInt(id) <= 9223372036854775807n;

export function signSession(id: string, secret: string, now = Date.now()): string {
	if (!isSnowflake(id) || !secret) throw new Error('Invalid session configuration');
	const payload = Buffer.from(JSON.stringify({ id, exp: Math.floor(now / 1000) + SESSION_AGE })).toString('base64url');
	return payload + '.' + createHmac('sha256', secret).update(payload).digest('base64url');
}

export function readSession(cookie: string | undefined, secret: string, now = Date.now()): { id: string } | null {
	if (!cookie || !secret) return null;
	const parts = cookie.split('.');
	if (parts.length !== 2) return null;
	const expected = createHmac('sha256', secret).update(parts[0]).digest();
	let actual: Buffer;
	try { actual = Buffer.from(parts[1], 'base64url'); } catch { return null; }
	if (actual.length !== expected.length || !timingSafeEqual(actual, expected)) return null;
	try {
		const data = JSON.parse(Buffer.from(parts[0], 'base64url').toString());
		return typeof data.id === 'string' && isSnowflake(data.id) && Number.isSafeInteger(data.exp) && data.exp > Math.floor(now / 1000) ? { id: data.id } : null;
	} catch { return null; }
}
