<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { greet } from "@/lib/tauri-commands";

const { t } = useI18n();
const greeting = ref("");

onMounted(async () => {
  // Bridge-works test (removed in Phase 0 when the probe command round-trips).
  try {
    greeting.value = await greet("you");
  } catch (e) {
    greeting.value = String(e);
  }
});
</script>

<template>
  <section class="mx-auto max-w-xl px-6 py-10">
    <h1 class="text-xl font-semibold">{{ t("home.title") }}</h1>
    <p class="mt-2 text-sm text-muted-foreground">{{ t("home.body") }}</p>
    <p class="mt-6 text-xs text-muted-foreground">
      {{ t("home.bridge") }} <code class="rounded bg-accent px-1.5 py-0.5">{{ greeting }}</code>
    </p>
  </section>
</template>
