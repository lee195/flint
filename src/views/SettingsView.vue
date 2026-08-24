<script setup lang="ts">
import { useI18n } from "vue-i18n";

const { t } = useI18n();
</script>

<template>
  <section class="mx-auto max-w-xl px-6 py-10">
    <h1 class="text-xl font-semibold">{{ t("settings.title") }}</h1>
    <p class="mt-2 text-sm text-muted-foreground">{{ t("settings.body") }}</p>
  </section>
</template>
