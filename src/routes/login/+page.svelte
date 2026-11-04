<script lang="ts">
 import { messages } from '#lib/messages.js';
 let { data } = $props();
 let m = $derived(messages[data.locale]);
</script>
<svelte:head><title>{m.login} · closure</title></svelte:head>
<div class="login"><div class="login-card"><div class="brand">closure</div><h1>{m.title}</h1><a class="primary" href="/auth/discord">{m.login}</a>{#if data.devLogin}<form method="POST" action="?/dev"><button class="secondary">{m.devLogin}</button></form>{/if}</div></div>
