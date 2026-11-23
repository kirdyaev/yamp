import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface TokenStatus {
  hasToken: boolean;
  last4: string | null;
}

const badgeEl = document.getElementById("settings-status-badge") as HTMLSpanElement;
const formEl = document.getElementById("token-form") as HTMLFormElement;
const inputEl = document.getElementById("token-input") as HTMLInputElement;
const clearBtn = document.getElementById("clear-token-btn") as HTMLButtonElement;
const feedbackEl = document.getElementById("settings-feedback") as HTMLElement;

function showFeedback(message: string, isError = false): void {
  feedbackEl.textContent = message;
  feedbackEl.classList.remove("hidden");
  feedbackEl.classList.toggle("error", isError);
}

function applyStatus(status: TokenStatus): void {
  if (status.hasToken) {
    badgeEl.textContent = status.last4 ? `Token saved ••••${status.last4}` : "Token saved";
    badgeEl.classList.remove("badge-warn");
    badgeEl.classList.add("badge-ok");
    clearBtn.classList.remove("hidden");
  } else {
    badgeEl.textContent = "Not configured";
    badgeEl.classList.remove("badge-ok");
    badgeEl.classList.add("badge-warn");
    clearBtn.classList.add("hidden");
  }
}

formEl.addEventListener("submit", async (e) => {
  e.preventDefault();
  const token = inputEl.value.trim();
  if (!token) {
    showFeedback("Please paste a valid OAuth token.", true);
    return;
  }

  try {
    const status = await invoke<TokenStatus>("save_token", { token });
    inputEl.value = "";
    applyStatus(status);
    const suffix = status.last4 ? ` (••••${status.last4})` : "";
    showFeedback(`Token saved to macOS Keychain${suffix}.`);
  } catch (err) {
    inputEl.value = "";
    showFeedback(String(err), true);
  }
});

clearBtn.addEventListener("click", async () => {
  try {
    const status = await invoke<TokenStatus>("clear_token");
    inputEl.value = "";
    applyStatus(status);
    showFeedback("Token removed from macOS Keychain.");
  } catch (err) {
    showFeedback(String(err), true);
  }
});

async function init(): Promise<void> {
  await listen<TokenStatus>("token-status-changed", (event) => {
    applyStatus(event.payload);
  });
  try {
    const status = await invoke<TokenStatus>("get_token_status");
    applyStatus(status);
  } catch (err) {
    showFeedback(String(err), true);
  }
}

void init();
