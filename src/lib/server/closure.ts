import * as env from '$app/env/private';

export type Guild = { id: string; name: string; icon: string | null };
export type Access = { level: number; level_name: string; allowed: boolean; reason: string };
export type ModuleConfig = { enabled?: boolean | null; [key: string]: unknown };
export type Config = { modules?: Record<string, ModuleConfig | null> | null; [key: string]: unknown };
export type History = { revision: number; created_at_unix: number; actor: string | null; document: Config };
export type Schema = { title?: string; properties?: Record<string, Schema>; anyOf?: Schema[]; [key: string]: unknown };
export type Directory = { channels: { id: string; name: string; kind: string; position: number; parent_id: string | null }[]; roles: { id: string; name: string; color: number; position: number; managed: boolean }[] };
export type ApiIssue = { path: string; code: string; message: string };
export class ClosureError extends Error {
	constructor(public status: number, public code: string, message: string, public issues: ApiIssue[] = []) { super(message); this.name = 'ClosureError'; }
}
export function closure(fetcher: typeof fetch = fetch) {
	const base = env.CLOSURE_API_URL;
	const token = env.CLOSURE_ADMIN_TOKEN;
	if (!base || !token) throw new Error('Missing required environment variables: CLOSURE_API_URL or CLOSURE_ADMIN_TOKEN');
	const clean = (value: string) => value.split(token).join('[redacted]');
	async function request<T>(path: string, init: RequestInit = {}): Promise<{ data: T; etag: string | null }> {
		let response: Response;
		try { response = await fetcher(new URL(path, base), { ...init, headers: { Accept: 'application/json', Authorization: `Bearer ${token}`, ...init.headers } }); }
		catch { throw new ClosureError(503, 'unavailable', 'Bot connection unavailable'); }
		const raw = await response.text();
		let body: any;
		try { body = JSON.parse(clean(JSON.stringify(JSON.parse(raw)))); } catch { body = null; }
		if (!response.ok) {
			const e = body?.error;
			throw new ClosureError(response.status, typeof e?.code === 'string' ? e.code : 'http_error', typeof e?.message === 'string' ? e.message : 'Bot API request failed', Array.isArray(e?.issues) ? e.issues : []);
		}
		if (body === null) throw new ClosureError(502, 'invalid_response', 'Invalid Bot API response');
		return { data: body as T, etag: response.headers.get('etag') === null ? null : clean(response.headers.get('etag')!) };
	}
	const guild = (id: string) => `/v1/guilds/${encodeURIComponent(id)}`;
	return {
		guilds: async () => (await request<Guild[]>('/v1/guilds')).data,
		directory: async (id: string) => (await request<Directory>(guild(id) + '/directory')).data,
		access: async (id: string, user: string) => (await request<Access>(guild(id) + '/access/' + encodeURIComponent(user))).data,
		config: async (id: string) => { const r = await request<Config>(guild(id) + '/config'); if (!r.etag) throw new ClosureError(502, 'missing_etag', 'Bot API response has no ETag'); return { data: r.data, etag: r.etag }; },
		putConfig: async (id: string, config: Config, etag: string, actor: string) => (await request<{ revision: number }>(guild(id) + '/config', { method: 'PUT', headers: { 'Content-Type': 'application/json', 'If-Match': etag, 'X-Closure-Actor': actor }, body: JSON.stringify(config) })).data,
		validate: async (id: string, config: Config) => (await request<{ valid: boolean; effective: Config }>(guild(id) + '/config/validate', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(config) })).data,
		effective: async (id: string) => (await request<Config>(guild(id) + '/config/effective')).data,
		history: async (id: string) => (await request<History[]>(guild(id) + '/config/history?limit=4')).data,
		schema: async (locale: string) => (await request<Schema>('/v1/schema?locale=' + encodeURIComponent(locale))).data
	};
}
