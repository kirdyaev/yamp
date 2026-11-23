import Webamp, { Track as WebampTrack } from "webamp";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";

interface TokenStatus {
  hasToken: boolean;
  last4: string | null;
}

interface ApiTrack {
  id: string;
  title: string;
  artist: string;
  album: string;
  duration: number;
  streamUrl: string;
}

interface PlayTracksPayload {
  tracks: ApiTrack[];
  startIndex: number;
}

const DOCK_HEIGHT = 22;
const WINAMP_WIDTH = 275;

const dockEl = document.getElementById("yamamp-dock") as HTMLElement;
const stageEl = document.getElementById("player-stage") as HTMLElement;
const dockMlBtn = document.getElementById("dock-ml-btn") as HTMLButtonElement;
const dockCfgBtn = document.getElementById("dock-cfg-btn") as HTMLButtonElement;
const dockStatusBtn = document.getElementById("dock-status-btn") as HTMLButtonElement;
const dockStatusText = document.getElementById("dock-status-text") as HTMLSpanElement;

let webamp: Webamp | null = null;
let hasToken = false;
let lastWindowHeight = 544;

function toWebampTrack(t: ApiTrack): WebampTrack {
  const url = t.streamUrl && t.streamUrl.startsWith("yamamp://")
    ? t.streamUrl
    : `yamamp://localhost/stream/${encodeURIComponent(t.id)}`;
  return {
    metaData: {
      artist: t.artist || "Unknown Artist",
      title: t.title || "Untitled",
      album: t.album || undefined,
    },
    url,
    duration: t.duration > 0 ? t.duration : undefined,
  };
}

async function ensureAudioContextRunning(): Promise<void> {
  const mediaCtx = (
    webamp as unknown as { media?: { _context?: AudioContext } } | null
  )?.media?._context;
  if (mediaCtx && mediaCtx.state === "suspended") {
    try {
      await mediaCtx.resume();
    } catch {
      // Ignore if resume fails before gesture
    }
  }
}

async function playInWebamp(tracks: ApiTrack[], startIndex = 0): Promise<void> {
  if (!webamp || tracks.length === 0) return;
  const safeStart = Math.max(0, Math.min(startIndex, tracks.length - 1));
  const slice = tracks.slice(safeStart).map(toWebampTrack);
  await ensureAudioContextRunning();
  webamp.setTracksToPlay(slice);
  window.setTimeout(() => {
    if (!webamp) return;
    void ensureAudioContextRunning();
    if (webamp.getMediaStatus() !== "PLAYING") {
      webamp.play();
    }
  }, 180);
}

function queueInWebamp(track: ApiTrack): void {
  if (!webamp) return;
  webamp.appendTracks([toWebampTrack(track)]);
}

function appendInWebamp(tracks: ApiTrack[]): void {
  if (!webamp || tracks.length === 0) return;
  webamp.appendTracks(tracks.map(toWebampTrack));
}

async function openLibrary(): Promise<void> {
  try {
    await invoke("open_library_window");
  } catch {
    // Ignore
  }
}

async function openSettings(): Promise<void> {
  try {
    await invoke("open_settings_window");
  } catch {
    // Ignore
  }
}

function applyTokenStatus(status: TokenStatus): void {
  hasToken = status.hasToken;
  if (status.hasToken) {
    dockStatusText.textContent = "YANDEX READY";
    dockStatusBtn.classList.remove("dock-status-warn");
    dockStatusBtn.classList.add("dock-status-ok");
    dockStatusBtn.title = "Open Media Library (Cmd+L)";
  } else {
    dockStatusText.textContent = "NO TOKEN";
    dockStatusBtn.classList.remove("dock-status-ok");
    dockStatusBtn.classList.add("dock-status-warn");
    dockStatusBtn.title = "Configure OAuth token in Settings";
  }
}

function isInteractiveControl(target: HTMLElement): boolean {
  if (
    target.closest(
      [
        "button",
        "input",
        "a",
        "#option",
        "#option-context",
        "#minimize",
        "#shade",
        "#close",
        "#equalizer-shade",
        "#equalizer-close",
        "#playlist-shade-button",
        "#playlist-close-button",
        "#webamp-context-menu",
        ".context-menu",
        "#eject",
        ".playlist-eject-button",
        "#playlist-resize-target",
        ".track-cell",
        ".playlist-scrollbar",
        "#playlist-add-menu",
        "#playlist-remove-menu",
        "#playlist-selection-menu",
        "#playlist-misc-menu",
        "#playlist-list-menu",
        ".band",
        "#volume",
        "#balance",
        "#position",
        ".actions",
        ".windows",
        ".shuffle-repeat",
        "#clutter-bar",
        "#equalizer-button",
        "#playlist-button",
        "#on",
        "#auto",
        "#presets",
        "#plus12db",
        "#zerodb",
        "#minus12db",
        "#playlist-scroll-up-button",
        "#playlist-scroll-down-button",
        ".playlist-action-buttons",
        "#marquee",
      ].join(", ")
    )
  ) {
    return true;
  }
  return false;
}

function isOsDragRegion(target: HTMLElement): boolean {
  if (isInteractiveControl(target)) {
    return false;
  }
  if (
    target.id === "title-bar" ||
    Boolean(target.closest("#title-bar")) ||
    Boolean(target.closest(".title-bar")) ||
    Boolean(target.closest(".equalizer-top")) ||
    Boolean(target.closest(".playlist-top")) ||
    Boolean(target.closest("#playlist-window-shade")) ||
    Boolean(target.closest("#yamamp-dock")) ||
    target.classList.contains("draggable")
  ) {
    return true;
  }
  return false;
}

function installNativeWindowDragging(): void {
  const appWindow = getCurrentWindow();

  document.addEventListener(
    "mousedown",
    (e: MouseEvent) => {
      if (e.button !== 0) return;
      const target = e.target as HTMLElement | null;
      if (!target) return;
      if (!isOsDragRegion(target)) return;

      // Intercept before Webamp's WindowManager can detach sub-windows internally.
      e.preventDefault();
      e.stopPropagation();
      void appWindow.startDragging().catch(() => {});
    },
    true
  );

  // Hook Webamp eject buttons to open our Yandex Music Library window instead of OS file dialog.
  document.addEventListener(
    "click",
    (e: MouseEvent) => {
      const target = e.target as HTMLElement | null;
      if (!target) return;
      if (target.closest("#eject") || target.closest(".playlist-eject-button")) {
        e.preventDefault();
        e.stopPropagation();
        void openLibrary();
      }
    },
    true
  );

  // Keyboard shortcuts
  window.addEventListener("keydown", (e: KeyboardEvent) => {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "l") {
      e.preventDefault();
      void openLibrary();
    } else if ((e.metaKey || e.ctrlKey) && e.key === ",") {
      e.preventDefault();
      void openSettings();
    }
  });
}

function syncWindowChromeHeight(): void {
  const selectors = [
    "#main-window",
    "#equalizer-window",
    "#playlist-window",
    "#playlist-window-shade",
  ];
  let maxBottom = 0;
  for (const sel of selectors) {
    const el = document.querySelector<HTMLElement>(sel);
    if (!el) continue;
    const rect = el.getBoundingClientRect();
    if (rect.width > 0 && rect.height > 0) {
      maxBottom = Math.max(maxBottom, Math.ceil(rect.bottom));
    }
  }
  if (maxBottom < 14) return;
  const clampedBottom = Math.min(Math.max(maxBottom, 14), 760);
  const totalHeight = clampedBottom + DOCK_HEIGHT;
  dockEl.style.top = `${clampedBottom}px`;
  stageEl.style.height = `${totalHeight}px`;

  if (totalHeight !== lastWindowHeight) {
    lastWindowHeight = totalHeight;
    void getCurrentWindow()
      .setSize(new LogicalSize(WINAMP_WIDTH, totalHeight))
      .catch(() => {});
  }
}

async function initWebamp(): Promise<void> {
  const anchor = document.getElementById("webamp-anchor");
  if (!anchor || !Webamp.browserIsSupported()) return;

  webamp = new Webamp({
    windowLayout: {
      main: { position: { top: 0, left: 0 }, shadeMode: false, closed: false },
      equalizer: { position: { top: 116, left: 0 }, shadeMode: false, closed: false },
      playlist: {
        position: { top: 232, left: 0 },
        shadeMode: false,
        closed: false,
        size: { extraHeight: 6, extraWidth: 0 },
      },
    },
    filePickers: [
      {
        contextMenuName: "Yandex Music Library...",
        filePicker: async () => {
          await openLibrary();
          return [];
        },
        requiresNetwork: true,
      },
    ],
    zIndex: 20,
  });

  webamp.onWillClose((cancel) => {
    cancel();
    void invoke("quit_app");
  });
  webamp.onClose(() => {
    void invoke("quit_app");
  });
  webamp.onMinimize(() => {
    void invoke("minimize_app");
  });

  await webamp.renderInto(anchor);

  webamp.__onStateChange(() => {
    window.requestAnimationFrame(syncWindowChromeHeight);
  });
  window.requestAnimationFrame(syncWindowChromeHeight);
}

async function bootstrap(): Promise<void> {
  installNativeWindowDragging();

  dockMlBtn.addEventListener("click", () => void openLibrary());
  dockCfgBtn.addEventListener("click", () => void openSettings());
  dockStatusBtn.addEventListener("click", () => {
    if (hasToken) {
      void openLibrary();
    } else {
      void openSettings();
    }
  });

  await initWebamp();

  await listen<TokenStatus>("token-status-changed", (event) => {
    applyTokenStatus(event.payload);
  });
  await listen<PlayTracksPayload>("yamamp:play-tracks", (event) => {
    void playInWebamp(event.payload.tracks || [], event.payload.startIndex || 0);
  });
  await listen<ApiTrack>("yamamp:queue-track", (event) => {
    queueInWebamp(event.payload);
  });
  await listen<ApiTrack[]>("yamamp:append-tracks", (event) => {
    appendInWebamp(event.payload || []);
  });

  try {
    const status = await invoke<TokenStatus>("get_token_status");
    applyTokenStatus(status);
  } catch {
    applyTokenStatus({ hasToken: false, last4: null });
  }
}

void bootstrap();
