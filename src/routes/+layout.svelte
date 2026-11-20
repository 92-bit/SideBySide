<script lang="ts">
  // The two fonts ship inside the app (free, SIL Open Font License), so nothing is downloaded.
  import '@fontsource-variable/geist';
  import '@fontsource-variable/geist-mono';
  import '../app.css';
  let { children } = $props();
</script>

{@render children()}
