<script lang="ts">
 import favicon from '#lib/assets/favicon.svg';
 import '../app.css';
 let { children, data } = $props();
</script>
<svelte:head><link rel="icon" href={favicon} /><script>try{const m=document.cookie.match(/(?:^|; )theme=(dark|light)(?:;|$)/);if(m)document.documentElement.dataset.theme=m[1]}catch{}</script></svelte:head>
{@render children()}
