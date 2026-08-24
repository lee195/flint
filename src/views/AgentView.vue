<script setup lang="ts">
import { nextTick, onMounted, onUnmounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { open } from "@tauri-apps/plugin-dialog";
import {
  cancelAgent,
  getAgentStatus,
  pollAgentOutput,
  respondPermission,
  restoreSnapshot,
  runAgent,
  runSmokeTest,
  setWorkspace,
} from "@/lib/tauri-commands";
import type { AgentEvent, AgentStatusView } from "@/lib/types";

type Item =
  | { kind: "user"; text: string }
  | { kind: "text"; text: string }
  | { kind: "tool"; callId: string; tool: string; input: string; ok?: boolean }
  | { kind: "permission"; permissionId: string; permission: string; pattern: string }
  | { kind: "error"; message: string }
  | { kind: "info"; text: string };

const { t } = useI18n();
const status = ref<AgentStatusView | null>(null);
const items = ref<Item[]>([]);
const streaming = ref("");
const prompt = ref("");
const running = ref(false);
const busy = ref(false);
const error = ref("");
const pastedPath = ref("");
const smokeBusy = ref(false);
const restored = ref("");
const listEl = ref<HTMLElement | null>(null);
let pollTimer: ReturnType<typeof setInterval> | null = null;
let statusTimer: ReturnType<typeof setInterval> | null = null;

async function refreshStatus() {
  try {
    status.value = await getAgentStatus();
    running.value = status.value?.running ?? false;
    if (!running.value) stopPolling();
  } catch (e) {
    error.value = String(e);
  }
}

onMounted(async () => {
  await refreshStatus();
  statusTimer = setInterval(refreshStatus, 5000);
});

onUnmounted(() => {
  stopPolling();
  if (statusTimer) clearInterval(statusTimer);
});

function stopPolling() {
  if (pollTimer) {
    clearInterval(pollTimer);
    pollTimer = null;
  }
}

function scrollToBottom() {
  nextTick(() => {
    if (listEl.value) listEl.value.scrollTop = listEl.value.scrollHeight;
  });
}

function handleEvent(ev: AgentEvent) {
  switch (ev.kind) {
    case "Delta":
      streaming.value += ev.text;
      scrollToBottom();
      break;
    case "ToolCall":
      if (!items.value.some((i) => i.kind === "tool" && i.callId === ev.callId)) {
        items.value.push({ kind: "tool", callId: ev.callId, tool: ev.tool, input: ev.input });
        scrollToBottom();
      }
      break;
    case "ToolResult":
      {
        const it = items.value.find((i) => i.kind === "tool" && i.callId === ev.callId);
        if (it && it.kind === "tool") it.ok = ev.ok;
      }
      break;
    case "Permission":
      if (!items.value.some((i) => i.kind === "permission" && i.permissionId === ev.id)) {
        items.value.push({
          kind: "permission",
          permissionId: ev.id,
          permission: ev.permission,
          pattern: ev.pattern,
        });
        scrollToBottom();
      }
      break;
    case "Done":
      running.value = false;
      stopPolling();
      if (streaming.value) {
        items.value.push({ kind: "text", text: streaming.value });
        streaming.value = "";
      }
      if (ev.text) items.value.push({ kind: "text", text: ev.text });
      items.value.push({ kind: "info", text: t("agent.review") });
      scrollToBottom();
      break;
    case "Error":
      running.value = false;
      stopPolling();
      streaming.value = "";
      items.value.push({ kind: "error", message: ev.message });
      scrollToBottom();
      break;
  }
}

async function startPolling() {
  stopPolling();
  pollTimer = setInterval(async () => {
    try {
      const events = await pollAgentOutput();
      for (const ev of events) handleEvent(ev);
    } catch {
      // transient poll error — next tick retries
    }
  }, 150);
}

async function run() {
  const text = prompt.value.trim();
  if (!text || running.value || busy.value) return;
  error.value = "";
  restored.value = "";
  items.value = [];
  streaming.value = "";
  items.value.push({ kind: "user", text });
  prompt.value = "";
  busy.value = true;
  try {
    await runAgent(text);
    running.value = true;
    await startPolling();
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
  scrollToBottom();
}

async function stop() {
  try {
    await cancelAgent();
  } catch {
    // Done/Error event will finalize
  }
}

async function answer(permissionId: string, allow: boolean) {
  const idx = items.value.findIndex((i) => i.kind === "permission" && i.permissionId === permissionId);
  if (idx >= 0) {
    const p = items.value[idx] as Extract<Item, { kind: "permission" }>;
    items.value.splice(idx, 1, {
      kind: "info",
      text: `${p.permission}: ${allow ? t("agent.allow") : t("agent.deny")}`,
    });
  }
  try {
    await respondPermission(permissionId, allow);
  } catch (e) {
    error.value = String(e);
  }
}

async function chooseFolder() {
  try {
    const dir = await open({ directory: true });
    if (typeof dir === "string") {
      await setWorkspace(dir);
      await refreshStatus();
    }
  } catch (e) {
    error.value = String(e);
  }
}

async function usePastedPath() {
  const p = pastedPath.value.trim();
  if (!p) return;
  try {
    await setWorkspace(p);
    pastedPath.value = "";
    await refreshStatus();
  } catch (e) {
    error.value = String(e);
  }
}

async function doSmoke() {
  smokeBusy.value = true;
  error.value = "";
  try {
    await runSmokeTest();
    await refreshStatus();
  } catch (e) {
    error.value = String(e);
  } finally {
    smokeBusy.value = false;
  }
}

async function doRestore() {
  restored.value = "";
  error.value = "";
  try {
    await restoreSnapshot();
    restored.value = t("agent.restored");
  } catch (e) {
    error.value = String(e);
  }
}
</script>

<template>
  <section class="flex h-full flex-col px-6 py-6">
    <div class="flex items-center justify-between">
      <h1 class="text-lg font-semibold">{{ t("agent.title") }}</h1>
      <span v-if="running" class="text-xs text-muted-foreground">{{ t("agent.running") }}</span>
    </div>

    <p v-if="error" class="mt-3 rounded-md border border-red-300 bg-red-50 px-3 py-2 text-sm text-red-700">
      {{ error }}
    </p>
    <p v-if="restored" class="mt-3 rounded-md border border-emerald-300 bg-emerald-50 px-3 py-2 text-sm text-emerald-800">
      {{ restored }}
    </p>

    <!-- Capability gate -->
    <div
      v-if="status && !status.unlocked"
      class="mt-4 rounded-lg border border-amber-300 bg-amber-50 p-4"
    >
      <h2 class="text-base font-semibold">{{ t("agent.lockedTitle") }}</h2>
      <p v-if="status.lockedReason" class="mt-1 text-sm text-muted-foreground">
        {{ t("agent.lockedReason") }} {{ status.lockedReason }}
      </p>
      <div v-if="status.lastSmoke" class="mt-2 text-sm">
        <span
          :class="status.lastSmoke.passed ? 'text-emerald-700' : 'text-red-700'"
        >
          {{ status.lastSmoke.passed ? t("agent.smokePassed") : t("agent.smokeFailed") }}
        </span>
        <p class="mt-1 text-xs text-muted-foreground">{{ t("agent.smokeDetail") }} {{ status.lastSmoke.detail }}</p>
      </div>
      <button
        class="mt-3 rounded-md bg-foreground px-4 py-2 text-sm font-medium text-background disabled:opacity-50"
        :disabled="smokeBusy"
        @click="doSmoke"
      >
        {{ smokeBusy ? t("agent.smokeRunning") : t("agent.smokeRun") }}
      </button>
    </div>

    <!-- Workspace -->
    <div class="mt-4 flex flex-wrap items-center gap-2">
      <span class="text-sm text-muted-foreground">{{ t("agent.workspace") }}:</span>
      <code class="max-w-[300px] truncate rounded bg-accent px-2 py-1 text-xs">
        {{ status?.workspace || t("agent.noWorkspace") }}
      </code>
      <button
        class="rounded-md bg-foreground px-3 py-1.5 text-xs font-medium text-background"
        @click="chooseFolder"
      >
        {{ t("agent.chooseFolder") }}
      </button>
      <div class="flex items-center gap-1">
        <span class="text-xs text-muted-foreground">{{ t("agent.pasteHint") }}</span>
        <input
          v-model="pastedPath"
          class="w-56 rounded-md border border-border bg-background px-2 py-1 text-xs focus:outline-none focus:ring-2 focus:ring-ring"
          :placeholder="t('agent.pastePlaceholder')"
        />
        <button
          class="text-xs text-muted-foreground underline"
          :disabled="!pastedPath.trim()"
          @click="usePastedPath"
        >
          {{ t("agent.add") }}
        </button>
      </div>
    </div>

    <p v-if="status?.unlocked && !status?.workspace" class="mt-2 text-sm text-muted-foreground">
      {{ t("agent.noWorkspace") }}
    </p>

    <!-- Transcript -->
    <div ref="listEl" class="mt-4 flex-1 overflow-y-auto rounded-lg border border-border p-4">
      <p v-if="!items.length && !running" class="text-sm text-muted-foreground">
        {{ t("agent.empty") }}
      </p>
      <div v-for="(it, i) in items" :key="i" class="mb-3">
        <div
          v-if="it.kind === 'user'"
          class="ml-auto max-w-[85%] whitespace-pre-wrap rounded-lg bg-foreground px-3 py-2 text-sm text-background"
        >
          {{ it.text }}
        </div>
        <div
          v-else-if="it.kind === 'text'"
          class="max-w-[85%] whitespace-pre-wrap rounded-lg bg-accent px-3 py-2 text-sm"
        >
          {{ it.text }}
        </div>
        <div v-else-if="it.kind === 'info'" class="text-xs text-muted-foreground">— {{ it.text }} —</div>
        <div
          v-else-if="it.kind === 'tool'"
          class="inline-block rounded-md border border-border bg-accent px-3 py-2 text-xs"
        >
          <span class="font-medium">{{ it.tool }}</span>
          <span class="ml-2 text-muted-foreground">{{ t("agent.toolCall") }}</span>
          <pre v-if="it.input" class="mt-1 max-w-md truncate text-muted-foreground">{{ it.input }}</pre>
          <span
            v-if="it.ok !== undefined"
            class="ml-2"
            :class="it.ok ? 'text-emerald-700' : 'text-red-700'"
          >
            {{ it.ok ? t("agent.toolResult") : t("agent.toolFailed") }}
          </span>
        </div>
        <div
          v-else-if="it.kind === 'permission'"
          class="rounded-md border border-amber-300 bg-amber-50 p-3"
        >
          <p class="text-sm font-medium">{{ t("agent.permissionTitle") }}</p>
          <p class="mt-1 text-xs break-all text-muted-foreground">
            {{ it.permission }} — {{ it.pattern }}
          </p>
          <div class="mt-2 flex gap-2">
            <button
              class="rounded-md bg-foreground px-3 py-1.5 text-xs font-medium text-background"
              @click="answer(it.permissionId, true)"
            >
              {{ t("agent.allow") }}
            </button>
            <button
              class="rounded-md bg-accent px-3 py-1.5 text-xs font-medium"
              @click="answer(it.permissionId, false)"
            >
              {{ t("agent.deny") }}
            </button>
          </div>
        </div>
        <div
          v-else-if="it.kind === 'error'"
          class="rounded-md border border-red-300 bg-red-50 px-3 py-2 text-sm text-red-700"
        >
          {{ it.message }}
        </div>
      </div>
      <div
        v-if="running"
        class="max-w-[85%] whitespace-pre-wrap rounded-lg bg-accent px-3 py-2 text-sm"
      >
        {{ streaming }}<span class="animate-pulse">▍</span>
      </div>
    </div>

    <!-- Review / restore -->
    <div
      v-if="!running && status?.unlocked && status?.workspace"
      class="mt-3 flex items-center gap-3"
    >
      <button
        class="rounded-md bg-accent px-3 py-1.5 text-xs font-medium"
        @click="doRestore"
      >
        {{ t("agent.restore") }}
      </button>
    </div>

    <!-- Input -->
    <div class="mt-3 flex items-end gap-2">
      <textarea
        v-model="prompt"
        rows="2"
        class="min-h-[44px] flex-1 resize-none rounded-lg border border-border bg-background px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-ring"
        :placeholder="t('agent.promptPlaceholder')"
        :disabled="running || busy || !status?.unlocked"
        @keydown.enter.exact.prevent="run"
      ></textarea>
      <button
        v-if="!running"
        class="shrink-0 rounded-md bg-foreground px-4 py-2 text-sm font-medium text-background disabled:opacity-50"
        :disabled="busy || !prompt.trim() || !status?.unlocked || !status?.workspace"
        @click="run"
      >
        {{ t("agent.run") }}
      </button>
      <button v-else class="shrink-0 rounded-md bg-accent px-4 py-2 text-sm font-medium" @click="stop">
        {{ t("agent.stop") }}
      </button>
    </div>
  </section>
</template>
