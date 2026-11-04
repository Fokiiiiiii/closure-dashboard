import * as env from '$app/env/private';
export function required(name: keyof typeof env): string {
	const value = env[name];
	if (!value) throw new Error(`Missing required environment variable: ${name}`);
	return value;
}
