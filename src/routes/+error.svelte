<script lang="ts">
 import { page } from '$app/state';
 import { messages } from '#lib/messages.js';
 let m = $derived(messages[page.data.locale === 'en' ? 'en' : 'ja']);
</script>
<div class="login"><div class="login-card"><div class="brand">closure</div><h1>{page.status}</h1><p>{page.error?.message ?? m.error}</p><a href="/">{m.back}</a></div></div>
