<script setup lang="ts">
import { nextTick, onMounted, onUnmounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import {
  cancelChat,
  engineStatus,
  getChat,
  newChat,
  pollChatOutput,
  sendChat,
  startEngine,
} from "@/lib/tauri-commands";
import type { ChatMessage, EngineStatusView } from "@/lib/types";

const { t } = useI18n();
const messages = ref<ChatMessage[]>([]);
const model = ref<string | null>(null);
const draft = ref("");
const streaming = ref(false);
const streamed = ref("");
const error = ref("");
const engine = ref<EngineStatusView | null>(null);
const confirmingNew = ref(false);
const listEl = ref<HTMLElement | null>(null);
const inputEl = ref<HTMLTextAreaElement | null>(null);
let pollTimer: ReturnType<typeof setInterval> | null = null;
let engineTimer: ReturnType<typeof setInterval> | null = null;

async function refreshEngine() {
  try {
    engine.value = await engineStatus();
  } catch (e) {
    error.value = String(e);
  }
}

onMounted(async () => {
  try {
    const chat = await getChat();
    messages.value = chat.messages;
    model.value = chat.model;
  } catch (e) {
    error.value = String(e);
  }
  refreshEngine();
  engineTimer = setInterval(refreshEngine, 5000);
  scrollToBottom();
});

onUnmounted(() => {
  stopPolling();
  if (engineTimer) clearInterval(engineTimer);
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

async function send() {
  const text = draft.value.trim();
  if (!text || streaming.value) return;
  draft.value = "";
  error.value = "";
  messages.value.push({ role: "user", content: text, ts: Date.now() });
  streamed.value = "";
  streaming.value = true;
  scrollToBottom();
  try {
    await sendChat(text);
  } catch (e) {
    streaming.value = false;
    error.value = String(e);
    return;
  }
  pollTimer = setInterval(async () => {
    try {
      const events = await pollChatOutput();
      for (const ev of events) {
        if (ev.kind === "Delta") {
          streamed.value += ev.content;
          scrollToBottom();
        } else if (ev.kind === "Done") {
          stopPolling();
          streaming.value = false;
          const finalText = ev.full || streamed.value;
          if (finalText) {
            messages.value.push({ role: "assistant", content: finalText, ts: Date.now() });
          }
          streamed.value = "";
          scrollToBottom();
        } else if (ev.kind === "Error") {
          stopPolling();
          streaming.value = false;
          streamed.value = "";
          error.value = ev.message;
        }
      }
    } catch {
      // transient poll errors are ignored — the next tick retries
    }
  }, 150);
}

async function cancel() {
  try {
    await cancelChat();
  } catch {
    // the Done/Error event will finalize
  }
}

async function doNewChat() {
  try {
    await newChat();
    messages.value = [];
    streamed.value = "";
    confirmingNew.value = false;
    scrollToBottom();
  } catch (e) {
    error.value = String(e);
  }
}

async function doLaunch() {
  try {
    await startEngine();
    await new Promise((r) => setTimeout(r, 1500));
    await refreshEngine();
  } catch (e) {
    error.value = String(e);
  }
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === "Enter" && !e.shiftKey) {
    e.preventDefault();
    send();
  }
}
</script>

<template>
  <section class="flex h-full flex-col px-6 py-6">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-3">
        <h1 class="text-lg font-semibold">{{ t("chat.title") }}</h1>
        <span v-if="model" class="text-xs text-muted-foreground">
          {{ model }} · {{ t("chat.runsOnDevice") }}
        </span>
        <span
          v-if="engine?.health.state === 'Running'"
          class="inline-flex items-center gap-1.5 rounded-full bg-emerald-100 px-2 py-0.5 text-xs font-medium text-emerald-800"
        >
          <span class="h-1.5 w-1.5 rounded-full bg-emerald-500"></span>
        </span>
        <span
          v-else-if="engine"
          class="inline-flex items-center gap-1.5 rounded-full bg-amber-100 px-2 py-0.5 text-xs font-medium text-amber-800"
        >
          <span class="h-1.5 w-1.5 rounded-full bg-amber-500"></span>
          {{ t("chat.engineDown") }}
        </span>
      </div>
      <button class="text-xs text-muted-foreground underline" @click="confirmingNew = true">
        {{ t("chat.newChat") }}
      </button>
    </div>

    <!-- New-chat inline confirm (WKWebView rule: no modal dialogs) -->
    <div v-if="confirmingNew" class="mt-3 rounded-md border border-amber-300 bg-amber-50 p-3">
      <p class="text-sm">{{ t("chat.confirmNew") }}</p>
      <div class="mt-2 flex gap-2">
        <button
          class="rounded-md bg-foreground px-3 py-1.5 text-xs font-medium text-background"
          @click="doNewChat"
        >
          {{ t("chat.confirmYes") }}
        </button>
        <button
          class="rounded-md bg-accent px-3 py-1.5 text-xs font-medium"
          @click="confirmingNew = false"
        >
          {{ t("chat.confirmNo") }}
        </button>
      </div>
    </div>

    <!-- Engine-down banner -->
    <div
      v-if="engine && engine.health.state !== 'Running'"
      class="mt-3 flex items-center justify-between rounded-md border border-amber-300 bg-amber-50 px-3 py-2"
    >
      <p class="text-sm">{{ t("chat.engineDown") }}</p>
      <button
        class="rounded-md bg-foreground px-3 py-1.5 text-xs font-medium text-background"
        @click="doLaunch"
      >
        {{ t("chat.launch") }}
      </button>
    </div>

    <p v-if="error" class="mt-3 rounded-md border border-red-300 bg-red-50 px-3 py-2 text-sm text-red-700">
      {{ error }}
    </p>

    <!-- Messages -->
    <div
      ref="listEl"
      class="mt-4 flex-1 overflow-y-auto rounded-lg border border-border p-4"
    >
      <p v-if="!messages.length && !streaming" class="text-sm text-muted-foreground">
        {{ t("chat.empty") }}
      </p>
      <div
        v-for="(m, i) in messages"
        :key="i"
        class="mb-3 max-w-[85%] whitespace-pre-wrap rounded-lg px-3 py-2 text-sm"
        :class="
          m.role === 'user'
            ? 'ml-auto bg-foreground text-background'
            : 'bg-accent text-foreground'
        "
      >
        {{ m.content }}
      </div>
      <div
        v-if="streaming"
        class="mb-3 max-w-[85%] whitespace-pre-wrap rounded-lg bg-accent px-3 py-2 text-sm text-foreground"
      >
        {{ streamed }}<span class="animate-pulse">▍</span>
      </div>
    </div>

    <!-- Input -->
    <div class="mt-3 flex items-end gap-2">
      <textarea
        ref="inputEl"
        v-model="draft"
        rows="2"
        class="min-h-[44px] flex-1 resize-none rounded-lg border border-border bg-background px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-ring"
        :placeholder="t('chat.placeholder')"
        :disabled="streaming"
        @keydown="onKeydown"
      ></textarea>
      <button
        v-if="!streaming"
        class="shrink-0 rounded-md bg-foreground px-4 py-2 text-sm font-medium text-background disabled:opacity-50"
        :disabled="!draft.trim()"
        @click="send"
      >
        {{ t("chat.send") }}
      </button>
      <button
        v-else
        class="shrink-0 rounded-md bg-accent px-4 py-2 text-sm font-medium"
        @click="cancel"
      >
        {{ t("chat.stop") }}
      </button>
    </div>
  </section>
</template>
