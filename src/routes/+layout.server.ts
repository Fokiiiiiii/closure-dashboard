import type { LayoutServerLoad } from './$types';
export const load: LayoutServerLoad = ({ locals, cookies }) => ({ locale: locals.locale, user: locals.user, theme: cookies.get('theme') === 'dark' ? 'dark' : cookies.get('theme') === 'light' ? 'light' : null });
