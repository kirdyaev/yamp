import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

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

interface ApiPlaylist {
  kind: number;
  ownerId: string;
  title: string;
  trackCount: number;
}

type TabName = "search" | "playlists" | "likes" | "wave";

const tokenBadge = document.getElementById("token-badge") as HTMLSpanElement;
const noTokenView = document.getElementById("no-token-view") as HTMLElement;
const libraryContent = document.getElementById("library-content") as HTMLElement;
const statusBanner = document.getElementById("status-banner") as HTMLElement;

const searchForm = document.getElementById("search-form") as HTMLFormElement;
const searchInput = document.getElementById("search-input") as HTMLInputElement;
const searchPlayAllBtn = document.getElementById("search-play-all") as HTMLButtonElement;
const searchResultsEl = document.getElementById("search-results") as HTMLElement;

const playlistsRefreshBtn = document.getElementById("playlists-refresh") as HTMLButtonElement;
const playlistSelectedTitleEl = document.getElementById("playlist-selected-title") as HTMLElement;
const playlistPlayAllBtn = document.getElementById("playlist-play-all") as HTMLButtonElement;
const playlistsListEl = document.getElementById("playlists-list") as HTMLElement;
const playlistTracksEl = document.getElementById("playlist-tracks") as HTMLElement;

const likesRefreshBtn = document.getElementById("likes-refresh") as HTMLButtonElement;
const likesPlayAllBtn = document.getElementById("likes-play-all") as HTMLButtonElement;
const likesCountEl = document.getElementById("likes-count") as HTMLElement;
const likesResultsEl = document.getElementById("likes-results") as HTMLElement;

const waveStartBtn = document.getElementById("wave-start") as HTMLButtonElement;
const waveMoreBtn = document.getElementById("wave-more") as HTMLButtonElement;
const waveResultsEl = document.getElementById("wave-results") as HTMLElement;

let hasToken = false;
let activeTab: TabName = "search";

let searchTracks: ApiTrack[] = [];
let playlistTracks: ApiTrack[] = [];
let likedTracks: ApiTrack[] = [];
let waveTracks: ApiTrack[] = [];
let playlistsLoaded = false;
let likesLoaded = false;
let waveLoaded = false;

function formatDuration(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds || 0));
  const mins = Math.floor(s / 60);
  const rem = s % 60;
  return `${mins}:${rem.toString().padStart(2, "0")}`;
}

function showBanner(message: string | null): void {
  if (!message) {
    statusBanner.textContent = "";
    statusBanner.classList.add("hidden");
    return;
  }
  statusBanner.textContent = message;
  statusBanner.classList.remove("hidden");
}

async function playInWebamp(tracks: ApiTrack[], startIndex = 0): Promise<void> {
  if (tracks.length === 0) return;
  try {
    await invoke("play_tracks_in_player", { tracks, startIndex });
  } catch (err) {
    showBanner(String(err));
  }
}

async function queueInWebamp(track: ApiTrack): Promise<void> {
  try {
    await invoke("queue_track_in_player", { track });
  } catch (err) {
    showBanner(String(err));
  }
}

async function appendInWebamp(tracks: ApiTrack[]): Promise<void> {
  if (tracks.length === 0) return;
  try {
    await invoke("append_tracks_in_player", { tracks });
  } catch (err) {
    showBanner(String(err));
  }
}

function renderTrackList(container: HTMLElement, tracks: ApiTrack[], emptyText: string): void {
  container.innerHTML = "";
  if (tracks.length === 0) {
    const empty = document.createElement("div");
    empty.className = "empty-state";
    empty.textContent = emptyText;
    container.appendChild(empty);
    return;
  }

  tracks.forEach((track, idx) => {
    const row = document.createElement("div");
    row.className = "track-row";
    row.title = "Double-click to play in Yamamp";

    const num = document.createElement("span");
    num.className = "track-num";
    num.textContent = `${idx + 1}.`;

    const meta = document.createElement("div");
    meta.className = "track-meta";

    const title = document.createElement("div");
    title.className = "track-title";
    title.textContent = track.title;

    const sub = document.createElement("div");
    sub.className = "track-sub";
    sub.textContent = track.album ? `${track.artist} — ${track.album}` : track.artist;

    meta.appendChild(title);
    meta.appendChild(sub);

    const dur = document.createElement("span");
    dur.className = "track-duration";
    dur.textContent = formatDuration(track.duration);

    const actions = document.createElement("div");
    actions.className = "track-actions";

    const playBtn = document.createElement("button");
    playBtn.type = "button";
    playBtn.className = "btn btn-primary btn-xxs";
    playBtn.textContent = "▶";
    playBtn.title = "Play from this track";
    playBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      void playInWebamp(tracks, idx);
    });

    const queueBtn = document.createElement("button");
    queueBtn.type = "button";
    queueBtn.className = "btn btn-secondary btn-xxs";
    queueBtn.textContent = "+";
    queueBtn.title = "Append to Webamp playlist";
    queueBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      void queueInWebamp(track);
    });

    actions.appendChild(playBtn);
    actions.appendChild(queueBtn);

    row.addEventListener("dblclick", () => void playInWebamp(tracks, idx));

    row.appendChild(num);
    row.appendChild(meta);
    row.appendChild(dur);
    row.appendChild(actions);
    container.appendChild(row);
  });
}

async function openSettings(): Promise<void> {
  try {
    await invoke("open_settings_window");
  } catch (err) {
    showBanner(String(err));
  }
}

function applyTokenStatus(status: TokenStatus): void {
  hasToken = status.hasToken;
  if (status.hasToken) {
    tokenBadge.textContent = status.last4 ? `TOKEN ••••${status.last4}` : "TOKEN SAVED";
    tokenBadge.classList.remove("badge-warn");
    tokenBadge.classList.add("badge-ok");
    noTokenView.classList.add("hidden");
    libraryContent.classList.remove("hidden");
    showBanner(null);
    maybeLoadActiveTab();
  } else {
    tokenBadge.textContent = "NO TOKEN";
    tokenBadge.classList.remove("badge-ok");
    tokenBadge.classList.add("badge-warn");
    noTokenView.classList.remove("hidden");
    libraryContent.classList.add("hidden");
    playlistsLoaded = false;
    likesLoaded = false;
    waveLoaded = false;
    searchTracks = [];
    playlistTracks = [];
    likedTracks = [];
    waveTracks = [];
  }
}

function switchTab(tab: TabName): void {
  activeTab = tab;
  document.querySelectorAll<HTMLButtonElement>(".tab").forEach((btn) => {
    btn.classList.toggle("active", btn.dataset.tab === tab);
  });
  document.querySelectorAll<HTMLElement>(".tab-pane").forEach((pane) => {
    pane.classList.toggle("active", pane.id === `tab-${tab}`);
  });
  if (hasToken) {
    maybeLoadActiveTab();
  }
}

function maybeLoadActiveTab(): void {
  if (activeTab === "playlists" && !playlistsLoaded) {
    void loadPlaylists();
  } else if (activeTab === "likes" && !likesLoaded) {
    void loadLikes();
  } else if (activeTab === "wave" && !waveLoaded) {
    void loadWave(false);
  }
}

async function runSearch(): Promise<void> {
  const query = searchInput.value.trim();
  if (!query) return;
  showBanner(null);
  searchResultsEl.innerHTML = '<div class="empty-state">Searching...</div>';
  try {
    const res = await invoke<{ tracks: ApiTrack[] }>("search_tracks", { query, page: 0 });
    searchTracks = res.tracks || [];
    searchPlayAllBtn.disabled = searchTracks.length === 0;
    renderTrackList(searchResultsEl, searchTracks, "No tracks found.");
  } catch (err) {
    searchTracks = [];
    searchPlayAllBtn.disabled = true;
    renderTrackList(searchResultsEl, [], "Search failed.");
    showBanner(String(err));
  }
}

async function loadPlaylists(): Promise<void> {
  showBanner(null);
  playlistsListEl.innerHTML = '<div class="empty-state">Loading...</div>';
  try {
    const res = await invoke<{ playlists: ApiPlaylist[] }>("get_playlists");
    playlistsLoaded = true;
    const list = res.playlists || [];
    playlistsListEl.innerHTML = "";
    if (list.length === 0) {
      playlistsListEl.innerHTML = '<div class="empty-state">No playlists.</div>';
      return;
    }
    list.forEach((pl) => {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "playlist-item";
      btn.textContent = pl.title;

      const sub = document.createElement("span");
      sub.className = "playlist-item-sub";
      sub.textContent = `${pl.trackCount} tracks`;
      btn.appendChild(sub);

      btn.addEventListener("click", () => {
        playlistsListEl
          .querySelectorAll(".playlist-item")
          .forEach((el) => el.classList.remove("active"));
        btn.classList.add("active");
        void loadPlaylistTracks(pl);
      });
      playlistsListEl.appendChild(btn);
    });
  } catch (err) {
    playlistsListEl.innerHTML = '<div class="empty-state">Load failed.</div>';
    showBanner(String(err));
  }
}

async function loadPlaylistTracks(pl: ApiPlaylist): Promise<void> {
  showBanner(null);
  playlistSelectedTitleEl.textContent = pl.title;
  playlistTracksEl.innerHTML = '<div class="empty-state">Loading tracks...</div>';
  try {
    const res = await invoke<{ tracks: ApiTrack[] }>("get_playlist_tracks", {
      ownerId: pl.ownerId,
      kind: pl.kind,
    });
    playlistTracks = res.tracks || [];
    playlistPlayAllBtn.disabled = playlistTracks.length === 0;
    renderTrackList(playlistTracksEl, playlistTracks, "Playlist is empty.");
  } catch (err) {
    playlistTracks = [];
    playlistPlayAllBtn.disabled = true;
    renderTrackList(playlistTracksEl, [], "Failed to load tracks.");
    showBanner(String(err));
  }
}

async function loadLikes(): Promise<void> {
  showBanner(null);
  likesResultsEl.innerHTML = '<div class="empty-state">Loading «Мне нравится»...</div>';
  try {
    const res = await invoke<{ tracks: ApiTrack[]; total: number }>("get_liked_tracks");
    likesLoaded = true;
    likedTracks = res.tracks || [];
    likesPlayAllBtn.disabled = likedTracks.length === 0;
    likesCountEl.textContent =
      res.total > likedTracks.length
        ? `${likedTracks.length}/${res.total}`
        : `${likedTracks.length} tracks`;
    renderTrackList(likesResultsEl, likedTracks, "No liked tracks yet.");
  } catch (err) {
    likedTracks = [];
    likesPlayAllBtn.disabled = true;
    renderTrackList(likesResultsEl, [], "Failed to load liked tracks.");
    showBanner(String(err));
  }
}

async function loadWave(append: boolean): Promise<void> {
  showBanner(null);
  if (!append) {
    waveResultsEl.innerHTML = '<div class="empty-state">Loading «Моя волна»...</div>';
  }
  try {
    const lastId = append && waveTracks.length > 0 ? waveTracks[waveTracks.length - 1].id : null;
    const res = await invoke<{ tracks: ApiTrack[] }>("get_my_wave", { queue: lastId });
    waveLoaded = true;
    const incoming = res.tracks || [];
    if (append) {
      waveTracks = [...waveTracks, ...incoming];
      if (incoming.length > 0) {
        await appendInWebamp(incoming);
      }
    } else {
      waveTracks = incoming;
    }
    renderTrackList(waveResultsEl, waveTracks, "No wave tracks returned.");
  } catch (err) {
    if (!append) {
      waveTracks = [];
      renderTrackList(waveResultsEl, [], "Failed to load «Моя волна».");
    }
    showBanner(String(err));
  }
}

async function bootstrap(): Promise<void> {
  document.getElementById("open-settings-btn")?.addEventListener("click", () => void openSettings());
  document.getElementById("no-token-settings-btn")?.addEventListener("click", () => void openSettings());

  document.querySelectorAll<HTMLButtonElement>(".tab").forEach((btn) => {
    btn.addEventListener("click", () => {
      const tab = btn.dataset.tab as TabName | undefined;
      if (tab) switchTab(tab);
    });
  });

  searchForm.addEventListener("submit", (e) => {
    e.preventDefault();
    void runSearch();
  });
  searchPlayAllBtn.addEventListener("click", () => void playInWebamp(searchTracks, 0));

  playlistsRefreshBtn.addEventListener("click", () => void loadPlaylists());
  playlistPlayAllBtn.addEventListener("click", () => void playInWebamp(playlistTracks, 0));

  likesRefreshBtn.addEventListener("click", () => void loadLikes());
  likesPlayAllBtn.addEventListener("click", () => void playInWebamp(likedTracks, 0));

  waveStartBtn.addEventListener("click", async () => {
    if (waveTracks.length === 0) {
      await loadWave(false);
    }
    await playInWebamp(waveTracks, 0);
  });
  waveMoreBtn.addEventListener("click", () => void loadWave(true));

  renderTrackList(searchResultsEl, [], "Enter a query and press Go.");
  renderTrackList(playlistTracksEl, [], "Select a playlist.");

  await listen<TokenStatus>("token-status-changed", (event) => {
    applyTokenStatus(event.payload);
  });

  try {
    const status = await invoke<TokenStatus>("get_token_status");
    applyTokenStatus(status);
  } catch (err) {
    applyTokenStatus({ hasToken: false, last4: null });
    showBanner(String(err));
  }
}

void bootstrap();
