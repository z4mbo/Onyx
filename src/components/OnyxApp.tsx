import {
  SignInButton,
  SignUpButton,
  UserButton,
  useClerk,
  useUser,
} from "@clerk/react";
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";

import type {
  AppSettings,
  CapabilityId,
  CapabilityRoute,
  CodexAccountStatus,
  CodexDeviceLoginStart,
  CodexRateLimits,
  ModelOption,
  ModelSelection,
  OnyxProfile,
  ProviderId,
  ReasoningEffort,
  SearchHistoryItem,
  TtsVoiceOption,
  VoiceSettings,
} from "../types";
import {
  applyBackendSettings,
  beginChatgptDeviceLogin,
  beginChatgptLogin,
  beginOpenRouterOAuth,
  chatgptAccountStatus,
  chatgptRateLimits,
  disconnectChatgpt,
  disconnectProvider,
  errorMessage,
  fallbackModels,
  getPlatform,
  hideWindow,
  isTauri,
  listModels,
  listTtsVoices,
  onOpenRouterAuth,
  openExternal,
  previewTts,
  providerConnectionStatus,
  providerLabel,
  quitApp,
  saveProviderApiKey,
  saveTtsConfig,
} from "../lib/api";
import {
  clearSearchHistory,
  consumeDictationError,
  loadJourneyStage,
  loadProfile,
  loadRoutes,
  loadSearchHistory,
  loadSettings,
  loadVoiceSettings,
  resetJourney,
  saveJourneyStage,
  saveProfile,
  saveRoutes,
  saveSettings,
  saveVoiceSettings,
  storageKeys,
  type JourneyStage,
} from "../lib/storage";
import { speakText, stopSpeech, systemVoices, toBackendTtsConfig } from "../lib/voice";

type DashboardSection = "home" | "history" | "dictation" | "agent" | "models" | "billing";
type Notice = { kind: "ok" | "error" | "info"; text: string };
type ClerkUserLike = {
  id: string;
  firstName: string | null;
  lastName: string | null;
  primaryEmailAddress?: { emailAddress: string } | null;
};
type ClerkSession = {
  clerkLoaded: boolean;
  isSignedIn: boolean;
  user: ClerkUserLike | null;
  signOut: () => Promise<unknown>;
};

const CLERK_CONFIGURED = Boolean(String(import.meta.env.VITE_CLERK_PUBLISHABLE_KEY ?? "").trim());
const BILLING_CHECKOUT_URL = String(import.meta.env.VITE_BILLING_CHECKOUT_URL ?? "").trim();

const NAV_ITEMS: Array<{ id: DashboardSection; label: string; icon: IconName }> = [
  { id: "home", label: "Home", icon: "home" },
  { id: "history", label: "Cronologia", icon: "history" },
  { id: "dictation", label: "Dettatura", icon: "mic" },
  { id: "agent", label: "Agente", icon: "spark" },
  { id: "models", label: "Modelli", icon: "route" },
  { id: "billing", label: "Piano", icon: "card" },
];

const SECTION_TITLES: Record<DashboardSection, { title: string; copy: string }> = {
  home: { title: "Buongiorno", copy: "La voce è il tuo nuovo input." },
  history: { title: "Cronologia", copy: "Ricerche dell’agente conservate solo su questo dispositivo." },
  dictation: { title: "Dettatura", copy: "Parla in qualsiasi campo di testo e Onyx scrive per te." },
  agent: { title: "Agente", copy: "Ricerca sul web, mostra le fonti e legge la risposta ad alta voce." },
  models: { title: "Routing modelli", copy: "Scegli un modello diverso per ogni capacità e ordina i fallback." },
  billing: { title: "Piano", copy: "Usa modelli locali, le tue chiavi, oppure Onyx Managed." },
};

const CAPABILITIES: Array<{
  id: CapabilityId;
  label: string;
  copy: string;
  icon: IconName;
  live: boolean;
}> = [
  { id: "stt", label: "Trascrizione", copy: "Voce → testo", icon: "mic", live: true },
  { id: "web_search", label: "Ricerca web", copy: "Risposte con fonti", icon: "globe", live: true },
  { id: "computer", label: "Controllo computer", copy: "App e interfacce", icon: "monitor", live: false },
  { id: "files", label: "File", copy: "Lettura e modifica", icon: "file", live: false },
  { id: "tts", label: "Voce", copy: "Testo → voce", icon: "volume", live: true },
  { id: "images", label: "Immagini", copy: "Generazione visuale", icon: "image", live: false },
  { id: "video", label: "Video", copy: "Generazione video", icon: "video", live: false },
];

const PROVIDERS: ProviderId[] = [
  "openrouter",
  "openai",
  "chatgpt_codex",
  "local",
  "managed",
  "anthropic_api",
  "claude_subscription_agent_sdk",
];

const VOICES = [
  { id: "sky", name: "Sky", copy: "Chiara e luminosa" },
  { id: "dawn", name: "Dawn", copy: "Calda e naturale" },
  { id: "dusk", name: "Dusk", copy: "Profonda e calma" },
  { id: "jarvis", name: "Jarvis", copy: "Bassa e precisa" },
];

const ONBOARDING_STEPS = [
  { group: "Setup", title: "E se potessi parlare al tuo computer?", copy: "Onyx trasforma la voce in azioni naturali, senza interrompere ciò che stai facendo." },
  { group: "Setup", title: "Recupera tempo ogni giorno", copy: "Una richiesta vocale sostituisce copia, ricerca, cambio app e riscrittura." },
  { group: "Setup", title: "Come possiamo chiamarti?", copy: "Personalizziamo saluto, lingua e tono della tua esperienza." },
  { group: "Setup", title: "Scegli la tua lingua", copy: "Onyx può rilevarla automaticamente; potrai cambiarla in ogni momento." },
  { group: "Setup", title: "Due permessi essenziali", copy: "Il microfono serve ad ascoltare; l’accessibilità inserisce il testo nell’app attiva." },
  { group: "Agent mode", title: "Scegli la voce di Onyx", copy: "Le risposte dell’agente vengono mostrate e lette ad alta voce." },
  { group: "Agent mode", title: "Tieni premuto Ctrl + Alt", copy: "Parla mentre tieni premuti i tasti. Al rilascio Onyx cerca e risponde con le fonti." },
  { group: "Agent mode", title: "Chiedi qualcosa al web", copy: "Prova: “Quali sono le notizie più importanti di oggi?”" },
  { group: "Dictation mode", title: "Scrivi ovunque con la voce", copy: "Tieni premuto Ctrl + Shift in un campo di testo. Al rilascio, Onyx trascrive." },
  { group: "Dictation mode", title: "Un modello per ogni capacità", copy: "Scegli provider, modello principale e fallback distinti per ricerca, voce e strumenti futuri." },
  { group: "Dictation mode", title: "Onyx è pronto", copy: "Continua gratis con modelli locali o BYOK, oppure scegli il piano Managed da €15 al mese." },
] as const;

export function OnyxApp() {
  return CLERK_CONFIGURED
    ? <ClerkOnyxApp />
    : <OnyxAppContent clerkLoaded isSignedIn={false} user={null} signOut={async () => undefined} />;
}

function ClerkOnyxApp() {
  const { isLoaded: clerkLoaded, isSignedIn, user } = useUser();
  const { signOut } = useClerk();
  return (
    <OnyxAppContent
      clerkLoaded={clerkLoaded}
      isSignedIn={Boolean(isSignedIn)}
      user={(user as ClerkUserLike | null | undefined) ?? null}
      signOut={() => signOut()}
    />
  );
}

function OnyxAppContent({ clerkLoaded, isSignedIn, user, signOut }: ClerkSession) {
  const [stage, setStage] = useState<JourneyStage>(() => loadJourneyStage());
  const [step, setStep] = useState(0);
  const [profile, setProfile] = useState<OnyxProfile>(() => loadProfile());
  const [settings, setSettings] = useState<AppSettings>(() => loadSettings());
  const [routes, setRoutes] = useState<CapabilityRoute[]>(() => loadRoutes());
  const [section, setSection] = useState<DashboardSection>("home");
  const [notice, setNotice] = useState<Notice | null>(() => {
    const error = consumeDictationError();
    return error ? { kind: "error", text: error } : null;
  });
  const [history, setHistory] = useState<SearchHistoryItem[]>(() => loadSearchHistory());
  const [platform, setPlatform] = useState("windows");
  const [catalogs, setCatalogs] = useState<Record<string, ModelOption[]>>({});
  const catalogRequests = useRef(new Set<string>());
  const [connections, setConnections] = useState<Partial<Record<ProviderId, boolean>>>({});
  const [micGranted, setMicGranted] = useState(false);

  useEffect(() => {
    if (!clerkLoaded) return;
    if (!isSignedIn || !user) {
      if (profile.authMode === "clerk" && stage !== "auth") {
        saveJourneyStage("auth");
        setStage("auth");
      }
      return;
    }
    if (stage !== "auth") return;
    const next: OnyxProfile = {
      ...profile,
      firstName: user.firstName || profile.firstName,
      lastName: user.lastName || profile.lastName,
      email: user.primaryEmailAddress?.emailAddress || profile.email,
      authMode: "clerk",
    };
    setProfile(next);
    saveProfile(next);
    saveJourneyStage("onboarding");
    setStage("onboarding");
  }, [clerkLoaded, isSignedIn, user?.id, stage]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    void getPlatform().then(setPlatform).catch(() => undefined);
    if (stage !== "app") return;
    void applyBackendSettings(settings).then((normalized) => {
      setSettings(normalized);
      saveSettings(normalized);
    }).catch((cause) => setNotice({ kind: "error", text: errorMessage(cause) }));
  }, [stage]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    const onStorage = (event: StorageEvent) => {
      if (event.key === storageKeys.searchHistory) setHistory(loadSearchHistory());
      if (event.key === storageKeys.lastError && event.newValue) {
        setNotice({ kind: "error", text: event.newValue });
      }
    };
    window.addEventListener("storage", onStorage);
    return () => window.removeEventListener("storage", onStorage);
  }, []);

  useEffect(() => {
    if (stage !== "app") return;
    let disposed = false;
    const keyProviders: ProviderId[] = ["openrouter", "openai", "anthropic_api"];
    void Promise.all(keyProviders.map(async (provider) => {
      try {
        const connected = await providerConnectionStatus(provider);
        if (!disposed) setConnections((current) => ({ ...current, [provider]: connected }));
      } catch {
        if (!disposed) setConnections((current) => ({ ...current, [provider]: false }));
      }
    }));
    let authCleanup: (() => void) | undefined;
    void onOpenRouterAuth((event) => {
      if (event.status === "connected") {
        setConnections((current) => ({ ...current, openrouter: true }));
        setNotice({ kind: "ok", text: event.message || "OpenRouter collegato." });
      } else if (event.status === "error") {
        setNotice({ kind: "error", text: event.message || "Accesso OpenRouter non riuscito." });
      }
    }).then((cleanup) => { authCleanup = cleanup; }).catch(() => undefined);
    return () => {
      disposed = true;
      authCleanup?.();
    };
  }, [stage]);

  async function enterPreview() {
    const next = { ...profile, authMode: "preview" as const, email: profile.email || "preview@onyx.local" };
    setProfile(next);
    saveProfile(next);
    saveJourneyStage("onboarding");
    setStage("onboarding");
  }

  function enterClerkAccount() {
    if (!isSignedIn || !user) return;
    const next: OnyxProfile = {
      ...profile,
      firstName: user.firstName || profile.firstName,
      lastName: user.lastName || profile.lastName,
      email: user.primaryEmailAddress?.emailAddress || profile.email,
      authMode: "clerk",
    };
    setProfile(next);
    saveProfile(next);
    saveJourneyStage("onboarding");
    setStage("onboarding");
  }

  async function leaveAccount() {
    resetJourney();
    if (isSignedIn) await signOut();
    setProfile(loadProfile());
    setStage("auth");
    setNotice(null);
  }

  async function finishOnboarding() {
    const nextProfile = {
      ...profile,
      firstName: profile.firstName.trim() || "Alex",
      language: profile.language || "it",
    };
    const nextSettings = { ...settings, language: nextProfile.language };
    saveProfile(nextProfile);
    saveSettings(nextSettings);
    saveRoutes(routes);
    saveJourneyStage("app");
    setProfile(nextProfile);
    setSettings(nextSettings);
    setStage("app");
    try {
      const normalized = await applyBackendSettings(nextSettings);
      setSettings(normalized);
      saveSettings(normalized);
    } catch (cause) {
      setNotice({ kind: "error", text: errorMessage(cause) });
    }
  }

  async function requestMicrophone() {
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      stream.getTracks().forEach((track) => track.stop());
      setMicGranted(true);
    } catch (cause) {
      setNotice({ kind: "error", text: `Microfono non disponibile: ${errorMessage(cause)}` });
    }
  }

  function commitSettings(next: AppSettings, message?: string) {
    setSettings(next);
    saveSettings(next);
    void applyBackendSettings(next).then((normalized) => {
      setSettings(normalized);
      saveSettings(normalized);
      if (message) setNotice({ kind: "ok", text: message });
    }).catch((cause) => setNotice({ kind: "error", text: errorMessage(cause) }));
  }

  function commitRoutes(next: CapabilityRoute[]) {
    setRoutes(next);
    saveRoutes(next);
    const stt = next.find((route) => route.capability === "stt")!.primary;
    const agent = next.find((route) => route.capability === "web_search")!.primary;
    commitSettings({
      ...settings,
      sttProvider: stt.provider,
      sttModel: stt.model,
      agentProvider: agent.provider,
      agentModel: agent.model,
    });
  }

  const loadCatalog = useCallback(async (provider: ProviderId, capability: CapabilityId) => {
    const key = `${provider}:${capability}`;
    if (catalogRequests.current.has(key)) return;
    catalogRequests.current.add(key);
    try {
      const models = await listModels(provider, capability);
      setCatalogs((current) => ({
        ...current,
        [key]: models.length ? models : fallbackModels(provider, capability),
      }));
    } catch {
      setCatalogs((current) => ({ ...current, [key]: fallbackModels(provider, capability) }));
    }
  }, []);

  const refreshProviderCatalogs = useCallback((provider: ProviderId) => {
    const capabilities = routes
      .filter((route) => route.primary.provider === provider || route.fallbacks.some((item) => item.provider === provider))
      .map((route) => route.capability);
    for (const capability of capabilities) catalogRequests.current.delete(`${provider}:${capability}`);
    setCatalogs((current) => Object.fromEntries(
      Object.entries(current).filter(([key]) => !key.startsWith(`${provider}:`)),
    ));
    capabilities.forEach((capability) => void loadCatalog(provider, capability));
  }, [loadCatalog, routes]);

  if (stage === "auth") {
    return (
      <AuthScreen
        notice={notice}
        clerkConfigured={CLERK_CONFIGURED}
        isSignedIn={isSignedIn}
        onAuthenticated={enterClerkAccount}
        onPreview={() => void enterPreview()}
      />
    );
  }

  if (stage === "onboarding") {
    return (
      <Onboarding
        step={step}
        profile={profile}
        settings={settings}
        micGranted={micGranted}
        notice={notice}
        onProfile={setProfile}
        onSettings={setSettings}
        onMicrophone={() => void requestMicrophone()}
        onBack={() => setStep((value) => Math.max(0, value - 1))}
        onNext={() => {
          saveProfile(profile);
          if (step === ONBOARDING_STEPS.length - 1) void finishOnboarding();
          else setStep((value) => value + 1);
        }}
      />
    );
  }

  const firstName = profile.firstName || "Alex";
  const sectionTitle = SECTION_TITLES[section];
  return (
    <main className="onyx-dashboard">
      <aside className="sidebar">
        <OnyxBrand />
        <nav className="side-nav" aria-label="Navigazione principale">
          {NAV_ITEMS.map((item) => (
            <button key={item.id} type="button" className={section === item.id ? "is-active" : ""} onClick={() => setSection(item.id)}>
              <Icon name={item.icon} /><span>{item.label}</span>
              {item.id === "models" && <i className="nav-dot" />}
            </button>
          ))}
        </nav>
        <div className="sidebar-bottom">
          <button className="upgrade-mini" type="button" onClick={() => setSection("billing")}>
            <span className="upgrade-orb"><i /></span>
            <span><strong>Onyx Managed</strong><small>€15 / mese</small></span>
            <span>›</span>
          </button>
          <div className="user-mini">
            {isSignedIn
              ? <span className="clerk-avatar"><UserButton /></span>
              : <span>{initials(profile)}</span>}
            <div><strong>{fullName(profile) || "Utente Onyx"}</strong><small>{profile.authMode === "preview" ? "Anteprima locale" : profile.email}</small></div>
          </div>
        </div>
      </aside>

      <section className="dashboard-main">
        <div className="window-drag-strip" data-tauri-drag-region />
        <header className="dashboard-topbar">
          <div><h1>{section === "home" ? `${sectionTitle.title}, ${firstName}` : sectionTitle.title}</h1><p>{sectionTitle.copy}</p></div>
          <div className="topbar-actions">
            <div className="top-shortcuts"><span><kbd>Ctrl</kbd><b>+</b><kbd>Shift</kbd> Dettatura</span><span><kbd>Ctrl</kbd><b>+</b><kbd>Alt</kbd> Agente</span></div>
            <button type="button" className="window-button" aria-label="Nascondi Onyx" onClick={() => void hideWindow("main")}>×</button>
          </div>
        </header>

        {notice && (
          <div className={`app-notice app-notice--${notice.kind}`} role={notice.kind === "error" ? "alert" : "status"}>
            <span>{notice.text}</span><button type="button" aria-label="Chiudi messaggio" onClick={() => setNotice(null)}>×</button>
          </div>
        )}

        <div className="dashboard-scroll">
          {section === "home" && <HomePanel history={history} connections={connections} setSection={setSection} />}
          {section === "history" && (
            <HistoryPanel history={history} onClear={() => { clearSearchHistory(); setHistory([]); }} />
          )}
          {section === "dictation" && (
            <DictationPanel settings={settings} platform={platform} onChange={(next) => commitSettings(next, "Impostazioni dettatura salvate.")} />
          )}
          {section === "agent" && (
            <AgentPanel settings={settings} setNotice={setNotice} onChange={(next) => commitSettings(next, "Impostazioni agente salvate.")} />
          )}
          {section === "models" && (
            <ModelsPanel
              routes={routes}
              catalogs={catalogs}
              connections={connections}
              onRoutes={commitRoutes}
              onNeedCatalog={loadCatalog}
              onProviderChanged={refreshProviderCatalogs}
              onConnections={setConnections}
              setNotice={setNotice}
            />
          )}
          {section === "billing" && <BillingPanel setNotice={setNotice} />}
        </div>

        <footer className="dashboard-footer">
          <span>Onyx 0.9 · dati locali</span>
          <div>
            <button type="button" onClick={() => void leaveAccount()}>Esci dall’account</button>
            <button type="button" className="danger" onClick={() => void quitApp()}>Esci da Onyx</button>
          </div>
        </footer>
      </section>
    </main>
  );
}

function AuthScreen({ notice, clerkConfigured, isSignedIn, onAuthenticated, onPreview }: {
  notice: Notice | null;
  clerkConfigured: boolean;
  isSignedIn: boolean;
  onAuthenticated: () => void;
  onPreview: () => void;
}) {
  return (
    <main className="auth-screen light-shell">
      <div className="window-drag-strip" data-tauri-drag-region />
      <section className="auth-pane">
        <OnyxBrand dark />
        <div className="auth-card">
          <span className="soft-label">BENVENUTO</span>
          <h1>Entra in Onyx</h1>
          <p>La tua voce, un modo più naturale di usare il computer.</p>
          {notice && <div className={`light-notice light-notice--${notice.kind}`}>{notice.text}</div>}
          {clerkConfigured && !isSignedIn && (
            <>
            <div className="clerk-auth-actions">
              <SignInButton mode="modal"><button className="light-primary" type="button">Accedi</button></SignInButton>
              <SignUpButton mode="modal"><button className="light-secondary" type="button">Crea un account</button></SignUpButton>
            </div>
            <div className="auth-separator"><span>metodi disponibili</span></div>
            <SignInButton mode="modal">
              <button className="social-button social-button--combined" type="button"><span className="social-marks"><GoogleMark /><AppleMark /></span><span>Google, Apple, email e altri</span></button>
            </SignInButton>
            </>
          )}
          {clerkConfigured && isSignedIn && (
            <div className="clerk-signed-in"><UserButton showName /><button className="light-primary" type="button" onClick={onAuthenticated}>Continua in Onyx</button></div>
          )}
          <button className="preview-button" type="button" onClick={onPreview}><Icon name="play" /><span><strong>Apri Anteprima locale</strong><small>Nessun account o pagamento · solo per test</small></span></button>
          <p className="auth-legal">Continuando accetti Termini e Privacy. L’accesso reale usa Clerk quando configurato.</p>
          <span className={`config-badge ${clerkConfigured ? "is-ready" : ""}`}><i />{clerkConfigured ? "Clerk collegato" : "Clerk non configurato"}</span>
        </div>
      </section>
      <SkyScene variant="auth" />
    </main>
  );
}

function Onboarding({ step, profile, settings, micGranted, notice, onProfile, onSettings, onMicrophone, onBack, onNext }: {
  step: number;
  profile: OnyxProfile;
  settings: AppSettings;
  micGranted: boolean;
  notice: Notice | null;
  onProfile: (profile: OnyxProfile) => void;
  onSettings: (settings: AppSettings) => void;
  onMicrophone: () => void;
  onBack: () => void;
  onNext: () => void;
}) {
  const current = ONBOARDING_STEPS[step];
  const setupProgress = Math.min(1, (step + 1) / 5);
  const agentProgress = step < 5 ? 0 : Math.min(1, (step - 4) / 3);
  const dictationProgress = step < 8 ? 0 : Math.min(1, (step - 7) / 3);
  return (
    <main className="onboarding light-shell">
      <div className="window-drag-strip" data-tauri-drag-region />
      <header className="onboarding-progress" data-tauri-drag-region>
        <OnyxBrand dark compact />
        <ProgressSegment label="Setup" value={setupProgress} active={current.group === "Setup"} />
        <ProgressSegment label="Agent mode" value={agentProgress} active={current.group === "Agent mode"} />
        <ProgressSegment label="Dictation mode" value={dictationProgress} active={current.group === "Dictation mode"} />
        <span className="step-count">{String(step + 1).padStart(2, "0")} / {ONBOARDING_STEPS.length}</span>
      </header>
      <section className="onboarding-body">
        <div className="onboarding-copy">
          <span className="soft-label">{current.group.toUpperCase()}</span>
          <h1>{current.title}</h1>
          <p>{current.copy}</p>
          {notice && <div className={`light-notice light-notice--${notice.kind}`}>{notice.text}</div>}
          <OnboardingControl
            step={step}
            profile={profile}
            settings={settings}
            micGranted={micGranted}
            onProfile={onProfile}
            onSettings={onSettings}
            onMicrophone={onMicrophone}
          />
          <div className="onboarding-actions">
            <button type="button" className="light-back" disabled={step === 0} onClick={onBack}>Indietro</button>
            <button type="button" className="light-primary next-button" onClick={onNext}>{step === ONBOARDING_STEPS.length - 1 ? "Apri Onyx" : "Continua"}<span>→</span></button>
          </div>
        </div>
        <OnboardingVisual step={step} settings={settings} />
      </section>
    </main>
  );
}

function OnboardingControl({ step, profile, settings, micGranted, onProfile, onSettings, onMicrophone }: {
  step: number;
  profile: OnyxProfile;
  settings: AppSettings;
  micGranted: boolean;
  onProfile: (profile: OnyxProfile) => void;
  onSettings: (settings: AppSettings) => void;
  onMicrophone: () => void;
}) {
  if (step === 2) {
    return <div className="name-grid"><label className="light-field"><span>Nome</span><input value={profile.firstName} placeholder="Alex" onChange={(event) => onProfile({ ...profile, firstName: event.target.value })} /></label><label className="light-field"><span>Cognome</span><input value={profile.lastName} placeholder="Rossi" onChange={(event) => onProfile({ ...profile, lastName: event.target.value })} /></label></div>;
  }
  if (step === 3) {
    return <div className="language-list"><button type="button" className={profile.language === "it" ? "is-selected" : ""} onClick={() => onProfile({ ...profile, language: "it" })}><span>🇮🇹</span><b>Italiano</b><i>Lingua principale</i></button><button type="button" className={profile.language === "en" ? "is-selected" : ""} onClick={() => onProfile({ ...profile, language: "en" })}><span>🇬🇧</span><b>English</b><i>English</i></button><button type="button" className={profile.language === "es" ? "is-selected" : ""} onClick={() => onProfile({ ...profile, language: "es" })}><span>🇪🇸</span><b>Español</b><i>Español</i></button></div>;
  }
  if (step === 4) {
    return <div className="permission-list"><button type="button" onClick={onMicrophone}><span className={micGranted ? "permission-icon is-ok" : "permission-icon"}><Icon name="mic" /></span><span><b>Microfono</b><small>{micGranted ? "Concesso e verificato" : "Necessario per ascoltarti"}</small></span><strong>{micGranted ? "✓" : "Consenti"}</strong></button><div className="permission-row"><span className="permission-icon"><Icon name="access" /></span><span><b>Accessibilità</b><small>Da verificare nelle impostazioni di sistema; serve per inserire il testo nell’app attiva.</small></span><strong>DA VERIFICARE</strong></div></div>;
  }
  if (step === 5) {
    return <VoicePicker value={settings.voicePreset} onChange={(voicePreset) => onSettings({ ...settings, voicePreset })} />;
  }
  if (step === 6 || step === 7) return <ShortcutDemo mode="agent" />;
  if (step === 8) return <ShortcutDemo mode="dictation" />;
  if (step === 9) {
    return <div className="provider-choices"><button type="button" className="is-selected"><span><Icon name="key" /></span><b>Le tue API</b><small>OpenRouter, OpenAI, Anthropic</small></button><button type="button"><span><Icon name="cpu" /></span><b>Locale</b><small>Endpoint sul tuo computer</small></button><button type="button"><span><Icon name="spark" /></span><b>Managed</b><small>Tutto incluso</small></button></div>;
  }
  if (step === 10) {
    return <div className="finish-plan"><span className="finish-check">✓</span><div><strong>Pronto per iniziare</strong><small>Il piano si sceglie dalla dashboard. Nessun addebito ora.</small></div></div>;
  }
  return null;
}

function OnboardingVisual({ step, settings }: { step: number; settings: AppSettings }) {
  return (
    <div className={`onboarding-visual visual-${step}`}>
      <SkyBackdrop />
      {step === 0 && <div className="floating-thoughts"><span>“Riassumi questa pagina”</span><span>“Scrivi una risposta gentile”</span><span>“Cerca le ultime notizie”</span></div>}
      {step === 1 && <div className="speed-visual"><div><small>Prima</small><strong>6</strong><span>passaggi</span></div><i>→</i><div className="is-fast"><small>Con Onyx</small><strong>1</strong><span>richiesta</span></div></div>}
      {step === 2 && <div className="profile-glass"><span className="avatar-cloud">{settings.voicePreset === "jarvis" ? "J" : "O"}</span><i /><i /><b /></div>}
      {step === 3 && <div className="language-bubbles"><span>Ciao</span><span>Hello</span><span>Hola</span><span>Bonjour</span></div>}
      {step === 4 && <div className="permission-visual"><div><Icon name="mic" /><span><b>Microfono</b><small>Solo mentre tieni premuto</small></span><strong>CHECK</strong></div><div><Icon name="access" /><span><b>Accessibilità</b><small>Verifica richiesta dal sistema</small></span><strong>SETUP</strong></div></div>}
      {(step === 5) && <div className="voice-orbs">{VOICES.map((voice) => <span key={voice.id} className={settings.voicePreset === voice.id ? "is-active" : ""}><i /><small>{voice.name}</small></span>)}</div>}
      {(step === 6 || step === 7) && <IslandDemo mode="agent" />}
      {step === 8 && <IslandDemo mode="dictation" />}
      {step === 9 && <div className="route-visual"><RouteMini icon="globe" label="Ricerca" model="GPT-4.1 mini" /><RouteMini icon="mic" label="Trascrizione" model="Whisper Large v3" /><RouteMini icon="monitor" label="Computer" model="Prossimamente" /></div>}
      {step === 10 && <div className="ready-visual"><span className="ready-orb"><i /><i /></span><strong>ONYX</strong><small>Ready when you are.</small></div>}
      <div className="visual-noise" />
    </div>
  );
}

function HomePanel({ history, connections, setSection }: { history: SearchHistoryItem[]; connections: Partial<Record<ProviderId, boolean>>; setSection: (section: DashboardSection) => void }) {
  const connectedCount = Object.values(connections).filter(Boolean).length;
  return (
    <div className="panel-stack">
      <section className="hero-dashboard-card">
        <div><span className="blue-eyebrow">ONYX VOICE LAYER</span><h2>Parla. Cerca. Scrivi.</h2><p>Tieni premuto un gesto, parla naturalmente e rilascia. Onyx resta fuori strada finché non ti serve.</p><div className="hero-actions"><button type="button"><kbd>Ctrl</kbd><b>+</b><kbd>Shift</kbd><span>Dettatura</span></button><button type="button"><kbd>Ctrl</kbd><b>+</b><kbd>Alt</kbd><span>Agente</span></button></div></div>
        <div className="hero-orb"><i /><i /><span className="tiny-island">|||||||</span></div>
      </section>
      <section className="stat-grid">
        <StatCard label="Ricerche" value={String(history.length)} copy="salvate localmente" icon="globe" />
        <StatCard label="Provider" value={String(connectedCount)} copy="pronti all’uso" icon="route" />
        <StatCard label="Dati cloud Onyx" value="0" copy="la cronologia resta qui" icon="lock" />
      </section>
      <section className="dashboard-card quick-grid-card">
        <div className="card-heading"><div><span className="blue-icon"><Icon name="spark" /></span><div><h3>Configurazione rapida</h3><p>Completa gli elementi essenziali</p></div></div><button type="button" onClick={() => setSection("models")}>Apri routing →</button></div>
        <div className="quick-grid"><QuickItem done label="Gesti hold-to-talk" copy="Ctrl+Shift / Ctrl+Alt" /><QuickItem done={connectedCount > 0} label="Provider AI" copy={connectedCount ? `${connectedCount} collegato` : "Collega una API"} /><QuickItem done label="Voce locale" copy="Risposte lette dal sistema" /><QuickItem label="Piano Managed" copy="Opzionale · €15/mese" /></div>
      </section>
    </div>
  );
}

function HistoryPanel({ history, onClear }: { history: SearchHistoryItem[]; onClear: () => void }) {
  return (
    <div className="panel-stack">
      <section className="local-banner"><Icon name="lock" /><div><strong>Privato per impostazione predefinita</strong><span>Le ricerche sono in localStorage su questo dispositivo; Onyx non le sincronizza.</span></div></section>
      <section className="dashboard-card history-card">
        <div className="card-heading"><div><span className="blue-icon"><Icon name="history" /></span><div><h3>Ricerche recenti</h3><p>{history.length} sessioni</p></div></div>{history.length > 0 && <button type="button" onClick={onClear}>Cancella tutto</button>}</div>
        {history.length === 0 ? <EmptyState icon="history" title="Ancora nessuna ricerca" copy="Tieni premuto Ctrl + Alt e chiedi qualcosa al web." /> : <div className="history-list">{history.map((item) => <article key={item.id}><div className="history-icon"><Icon name="globe" /></div><div><time>{formatDate(item.createdAt)}</time><h4>{item.query}</h4><p>{item.answer}</p><div className="history-meta"><span>{item.model}</span><span>{item.sources.length} fonti</span>{item.usage.cost != null && <span>${item.usage.cost.toFixed(4)}</span>}</div></div></article>)}</div>}
      </section>
    </div>
  );
}

function DictationPanel({ settings, platform, onChange }: { settings: AppSettings; platform: string; onChange: (settings: AppSettings) => void }) {
  const [draft, setDraft] = useState(settings);
  useEffect(() => setDraft(settings), [settings]);
  return (
    <div className="settings-columns">
      <section className="dashboard-card settings-card"><CardTitle icon="mic" title="Gesto di dettatura" copy="Mantieni premuto, poi rilascia per trascrivere" /><div className="shortcut-display"><kbd>{platform === "macos" ? "⌃" : "Ctrl"}</kbd><b>+</b><kbd>Shift</kbd><span>HOLD</span></div><p className="field-help">Il gesto usa solo i modificatori sinistri per evitare attivazioni accidentali.</p></section>
      <section className="dashboard-card settings-card"><CardTitle icon="sliders" title="Trascrizione" copy="Lingua e posizione dell’indicatore" /><label className="dark-field"><span>Lingua</span><select value={draft.language ?? ""} onChange={(event) => setDraft({ ...draft, language: event.target.value || null })}><option value="">Rilevamento automatico</option><option value="it">Italiano</option><option value="en">English</option><option value="es">Español</option><option value="fr">Français</option></select></label><label className="dark-field"><span>Posizione indicatore</span><select value={draft.overlayPosition} onChange={(event) => setDraft({ ...draft, overlayPosition: event.target.value as AppSettings["overlayPosition"] })}><option value="bottom_center">Basso · centro</option><option value="top_center">Alto · centro</option><option value="bottom_left">Basso · sinistra</option><option value="bottom_right">Basso · destra</option></select></label><button type="button" className="dark-primary" onClick={() => onChange(draft)}>Salva modifiche</button></section>
      <section className="dashboard-card settings-card wide-card"><CardTitle icon="wave" title="Indicatore contestuale" copy="Le onde reagiscono al volume e il badge mostra l’app attiva" /><div className="hud-preview"><span className="preview-app">C</span><i /><div>{[4, 8, 15, 22, 13, 8, 4].map((height, index) => <b key={index} style={{ height }} />)}</div><small>Google Chrome</small></div></section>
    </div>
  );
}

function AgentPanel({ settings, onChange, setNotice }: {
  settings: AppSettings;
  onChange: (settings: AppSettings) => void;
  setNotice: (notice: Notice | null) => void;
}) {
  const [draft, setDraft] = useState(settings);
  const [voiceDraft, setVoiceDraft] = useState<VoiceSettings>(() => loadVoiceSettings());
  const [voices, setVoices] = useState<TtsVoiceOption[]>(() => systemVoices().map((voice) => ({ ...voice, provider: "system" })));
  const [playing, setPlaying] = useState(false);
  const [previewBusy, setPreviewBusy] = useState(false);
  const [ttsConnections, setTtsConnections] = useState({ openrouter: false, openai: false });
  const [ttsModels, setTtsModels] = useState<ModelOption[]>(() => fallbackModels("openrouter", "tts"));
  const [modelsLoading, setModelsLoading] = useState(false);
  const previewToken = useRef(0);

  useEffect(() => setDraft(settings), [settings]);
  useEffect(() => {
    const refresh = () => {
      if (!isTauri && voiceDraft.provider === "system") {
        setVoices(systemVoices().map((voice) => ({ ...voice, provider: "system" })));
      }
    };
    refresh();
    window.speechSynthesis?.addEventListener("voiceschanged", refresh);
    return () => {
      previewToken.current += 1;
      window.speechSynthesis?.removeEventListener("voiceschanged", refresh);
      stopSpeech();
    };
  }, [voiceDraft.provider]);

  useEffect(() => {
    let disposed = false;
    void Promise.all([
      providerConnectionStatus("openrouter").catch(() => false),
      providerConnectionStatus("openai").catch(() => false),
    ]).then(([openrouter, openai]) => {
      if (!disposed) setTtsConnections({ openrouter, openai });
    });
    return () => { disposed = true; };
  }, []);

  useEffect(() => {
    if (voiceDraft.provider === "system") return;
    const provider = voiceDraft.provider;
    let disposed = false;
    setModelsLoading(true);
    void listModels(provider, "tts").then((models) => {
      if (!disposed) setTtsModels(models.length ? models : fallbackModels(provider, "tts"));
    }).catch(() => {
      if (!disposed) setTtsModels(fallbackModels(provider, "tts"));
    }).finally(() => {
      if (!disposed) setModelsLoading(false);
    });
    return () => { disposed = true; };
  }, [voiceDraft.provider]);

  useEffect(() => {
    let disposed = false;
    const provider = voiceDraft.provider;
    const selected = provider === "system" ? voiceDraft.voiceId : voiceDraft.cloudVoice;
    const fallback: TtsVoiceOption[] = provider === "system"
      ? systemVoices().map((voice) => ({ ...voice, provider: "system" }))
      : [{ id: selected || "alloy", name: selected || "Alloy", provider, language: null, local: false }];
    if (!isTauri) {
      setVoices(fallback);
      return () => { disposed = true; };
    }
    void listTtsVoices(provider, voiceDraft.model).then((options) => {
      if (disposed) return;
      const next = options.length ? options : fallback;
      setVoices(next);
      if (provider !== "system" && !next.some((voice) => voice.id === selected)) {
        setVoiceDraft((current) => current.provider === provider && current.model === voiceDraft.model
          ? { ...current, cloudVoice: next[0].id }
          : current);
      }
    }).catch(() => {
      if (!disposed) setVoices(fallback);
    });
    return () => { disposed = true; };
  }, [voiceDraft.provider, voiceDraft.model]);

  async function previewVoice() {
    if (previewBusy) return;
    if (playing && !isTauri) {
      previewToken.current += 1;
      stopSpeech();
      setPlaying(false);
      return;
    }
    const token = ++previewToken.current;
    const sample = "Ciao, sono Onyx. La voce è il modo più naturale di usare il tuo computer.";
    if (!isTauri && voiceDraft.provider === "system") {
      const started = speakText(sample, voiceDraft, draft.language, draft.voicePreset, {
        onStart: () => { if (token === previewToken.current) setPlaying(true); },
        onEnd: () => { if (token === previewToken.current) setPlaying(false); },
        onError: () => { if (token === previewToken.current) setPlaying(false); },
      });
      if (!started) setNotice({ kind: "error", text: "La sintesi vocale di sistema non è disponibile." });
      else setPlaying(true);
      return;
    }
    if (!isTauri) {
      setNotice({ kind: "info", text: "L'anteprima cloud Ã¨ disponibile nella build desktop." });
      return;
    }
    if (voiceDraft.provider !== "system" && !ttsConnections[voiceDraft.provider]) {
      setNotice({ kind: "info", text: `Collega ${voiceDraft.provider === "openrouter" ? "OpenRouter" : "OpenAI"} nella sezione Modelli prima dell’anteprima.` });
      return;
    }
    setPreviewBusy(true);
    setPlaying(true);
    try {
      await saveTtsConfig(toBackendTtsConfig(voiceDraft));
      const result = await previewTts(sample);
      if (token !== previewToken.current) return;
      if (result.warning) setNotice({ kind: "info", text: result.warning });
    } catch (cause) {
      if (token === previewToken.current) {
        setNotice({ kind: "error", text: `Anteprima TTS non riuscita: ${errorMessage(cause)}` });
      }
    } finally {
      if (token === previewToken.current) {
        setPreviewBusy(false);
        setPlaying(false);
      }
    }
  }

  async function saveVoice() {
    saveVoiceSettings(voiceDraft);
    if (isTauri) {
      try {
        await saveTtsConfig(toBackendTtsConfig(voiceDraft));
      } catch (cause) {
        setNotice({ kind: "error", text: errorMessage(cause) });
        return;
      }
    }
    onChange(draft);
    setNotice({ kind: "ok", text: "Voce, provider e velocità salvati sul dispositivo." });
  }

  const cloudModels = ttsModels.some((model) => model.id === voiceDraft.model)
    ? ttsModels
    : [{ id: voiceDraft.model, name: voiceDraft.model }, ...ttsModels];
  const isCloud = voiceDraft.provider !== "system";
  const selectedVoice = isCloud ? voiceDraft.cloudVoice : voiceDraft.voiceId;
  const visibleVoices: TtsVoiceOption[] = selectedVoice && !voices.some((voice) => voice.id === selectedVoice)
    ? [{ id: selectedVoice, name: selectedVoice, provider: voiceDraft.provider, language: null, local: voiceDraft.provider === "system" }, ...voices]
    : voices;

  return (
    <div className="settings-columns">
      <section className="dashboard-card settings-card">
        <CardTitle icon="spark" title="Agente di ricerca" copy="Per ora Onyx consulta il web: nessun controllo del computer" />
        <div className="feature-status"><span><i />Ricerca web con fonti</span><b>ATTIVO</b></div>
        <div className="feature-status is-future"><span><i />Computer e file</span><b>ROADMAP</b></div>
        <label className="toggle-row"><span><b>Leggi le risposte</b><small>Riproduce la risposta con la voce selezionata</small></span><input type="checkbox" checked={draft.speakResponses} onChange={(event) => setDraft({ ...draft, speakResponses: event.target.checked })} /></label>
      </section>
      <section className="dashboard-card settings-card">
        <CardTitle icon="brain" title="Ragionamento" copy="Più ragionamento può aumentare latenza e consumo" />
        <div className="reasoning-picker">{(["none", "low", "medium", "high", "xhigh"] as ReasoningEffort[]).map((value) => <button key={value} type="button" className={draft.reasoning === value ? "is-active" : ""} onClick={() => setDraft({ ...draft, reasoning: value })}>{value === "none" ? "Off" : value === "xhigh" ? "Max" : value[0].toUpperCase() + value.slice(1)}</button>)}</div>
        <button type="button" className="dark-primary" onClick={() => onChange(draft)}>Salva modifiche</button>
      </section>
      <section className="dashboard-card settings-card wide-card voice-settings-card">
        <CardTitle icon="volume" title="Voce dell’agente" copy="Scegli provider, voce installata e velocità di lettura" />
        <div className="tts-status-row">
          <span className="tts-status is-live"><i /><b>Sistema / Browser</b><small>Operativo · nessun costo API</small></span>
          <span className={`tts-status ${ttsConnections.openrouter ? "is-live" : ""}`}><i /><b>OpenRouter</b><small>{ttsConnections.openrouter ? "Operativo · modelli TTS cloud" : "Collega la tua API key"}</small></span>
          <span className={`tts-status ${ttsConnections.openai ? "is-live" : ""}`}><i /><b>OpenAI API</b><small>{ttsConnections.openai ? "Operativo · TTS diretto" : "Collega la tua API key"}</small></span>
        </div>
        <div className={`voice-config-grid ${isCloud ? "is-cloud" : ""}`}>
          <label className="dark-field">
            <span>Provider TTS</span>
            <select value={voiceDraft.provider} onChange={(event) => {
              const provider = event.target.value as VoiceSettings["provider"];
              const model = provider === "openai" ? "gpt-4o-mini-tts" : provider === "openrouter" ? "openai/gpt-4o-mini-tts-2025-12-15" : voiceDraft.model;
              setVoiceDraft({ ...voiceDraft, provider, model });
            }}>
              <option value="system">Sistema / Browser · attivo</option>
              <option value="openrouter" disabled={!ttsConnections.openrouter}>OpenRouter{ttsConnections.openrouter ? " · attivo" : " · collega API"}</option>
              <option value="openai" disabled={!ttsConnections.openai}>OpenAI API{ttsConnections.openai ? " · attivo" : " · collega API"}</option>
            </select>
          </label>
          {isCloud && <label className="dark-field">
            <span>Modello TTS</span>
            <select value={voiceDraft.model} disabled={modelsLoading} onChange={(event) => setVoiceDraft({ ...voiceDraft, model: event.target.value })}>
              {cloudModels.map((model) => <option value={model.id} key={model.id}>{model.name}</option>)}
            </select>
          </label>}
          <label className="dark-field">
            <span>{isCloud ? "Voce del modello" : "Voce installata"}</span>
            <select value={isCloud ? voiceDraft.cloudVoice : voiceDraft.voiceId} onChange={(event) => isCloud ? setVoiceDraft({ ...voiceDraft, cloudVoice: event.target.value }) : setVoiceDraft({ ...voiceDraft, voiceId: event.target.value })}>
              {!isCloud && <option value="">Automatica · lingua di Onyx</option>}
              {!isCloud && visibleVoices.map((voice) => <option key={voice.id} value={voice.id}>{voice.name}{voice.language ? ` · ${voice.language}` : ""}{voice.local ? " · locale" : ""}</option>)}
              {isCloud && visibleVoices.map((voice) => <option key={voice.id} value={voice.id}>{voice.name}{voice.language ? ` · ${voice.language}` : ""}</option>)}
            </select>
          </label>
          <label className="voice-rate-field">
            <span>Velocità <b>{voiceDraft.rate.toFixed(2).replace(/0$/, "")}×</b></span>
            <input type="range" min="0.6" max="1.4" step="0.05" value={voiceDraft.rate} onChange={(event) => setVoiceDraft({ ...voiceDraft, rate: Number(event.target.value) })} />
            <small><span>Più lenta</span><span>Naturale</span><span>Più veloce</span></small>
          </label>
        </div>
        <div className="voice-settings-actions">
          <button type="button" className={`voice-preview-button ${playing ? "is-playing" : ""}`} onClick={previewVoice}>
            <Icon name={playing ? "volume" : "play"} /><span>{previewBusy ? "Genero anteprima…" : playing ? "Interrompi anteprima" : "Ascolta anteprima"}</span>
          </button>
          <button type="button" className="dark-primary compact-save" onClick={() => void saveVoice()}>Salva voce</button>
        </div>
        <p className="voice-privacy-note"><Icon name="lock" /> La voce di sistema resta locale. OpenRouter e OpenAI usano la chiave custodita dal portachiavi; voci e velocità disponibili possono variare per modello. I modelli ElevenLabs, quando disponibili, si selezionano dal catalogo OpenRouter.</p>
      </section>
    </div>
  );
}

function ModelsPanel({ routes, catalogs, connections, onRoutes, onNeedCatalog, onProviderChanged, onConnections, setNotice }: {
  routes: CapabilityRoute[];
  catalogs: Record<string, ModelOption[]>;
  connections: Partial<Record<ProviderId, boolean>>;
  onRoutes: (routes: CapabilityRoute[]) => void;
  onNeedCatalog: (provider: ProviderId, capability: CapabilityId) => void;
  onProviderChanged: (provider: ProviderId) => void;
  onConnections: (connections: Partial<Record<ProviderId, boolean>>) => void;
  setNotice: (notice: Notice | null) => void;
}) {
  return (
    <div className="panel-stack">
      <section className="dashboard-card providers-card">
        <CardTitle icon="key" title="Provider" copy="API key e sessioni ChatGPT restano nel portachiavi o nel runtime ufficiale Codex" />
        <div className="provider-grid">
          <ProviderConnection provider="openrouter" connected={Boolean(connections.openrouter)} onConnection={(value) => { onConnections({ ...connections, openrouter: value }); onProviderChanged("openrouter"); }} setNotice={setNotice} />
          <ProviderConnection provider="openai" connected={Boolean(connections.openai)} onConnection={(value) => { onConnections({ ...connections, openai: value }); onProviderChanged("openai"); }} setNotice={setNotice} />
          <ChatGptCodexConnection onConnection={(value) => { onConnections({ ...connections, chatgpt_codex: value }); onProviderChanged("chatgpt_codex"); }} setNotice={setNotice} />
          <ProviderConnection provider="anthropic_api" connected={Boolean(connections.anthropic_api)} onConnection={(value) => { onConnections({ ...connections, anthropic_api: value }); onProviderChanged("anthropic_api"); }} setNotice={setNotice} />
          <InfoProvider name="Modelli locali" label="LOCALE" icon="cpu" copy="Predisposto per endpoint OpenAI-compatible. Il runtime non viene scaricato da questa schermata." />
          <InfoProvider name="Claude Pro / Max" label="SDK LOCALE" icon="spark" copy="Percorso Agent SDK sperimentale; richiede il runtime Claude autenticato sul computer." />
        </div>
      </section>
      <section className="route-heading"><div><span className="blue-icon"><Icon name="route" /></span><div><h3>Router per capacità</h3><p>Il modello principale è operativo. L’ordine dei fallback è già configurabile, ma l’esecuzione automatica arriverà con il router server-side.</p></div></div><span className="live-legend"><i />3 capacità attive oggi</span></section>
      <div className="route-grid">{CAPABILITIES.map((capability) => {
        const route = routes.find((item) => item.capability === capability.id)!;
        return <ModelRouteCard key={capability.id} definition={capability} route={route} catalogs={catalogs} onNeedCatalog={onNeedCatalog} onChange={(nextRoute) => onRoutes(routes.map((item) => item.capability === nextRoute.capability ? nextRoute : item))} />;
      })}</div>
    </div>
  );
}

function ModelRouteCard({ definition, route, catalogs, onNeedCatalog, onChange }: {
  definition: typeof CAPABILITIES[number];
  route: CapabilityRoute;
  catalogs: Record<string, ModelOption[]>;
  onNeedCatalog: (provider: ProviderId, capability: CapabilityId) => void;
  onChange: (route: CapabilityRoute) => void;
}) {
  const fallbackProviders = route.fallbacks.map((selection) => selection.provider).join(",");
  useEffect(() => {
    onNeedCatalog(route.primary.provider, definition.id);
    route.fallbacks.forEach((selection) => onNeedCatalog(selection.provider, definition.id));
  }, [definition.id, fallbackProviders, onNeedCatalog, route.primary.provider]);
  const modelsFor = (selection: ModelSelection) => {
    const base = catalogs[`${selection.provider}:${definition.id}`] ?? fallbackModels(selection.provider, definition.id);
    return base.some((model) => model.id === selection.model) ? base : [{ id: selection.model, name: selection.model }, ...base];
  };
  const providerOptions = providerOptionsFor(definition.id);
  const changeProvider = (index: number | "primary", provider: ProviderId) => {
    onNeedCatalog(provider, definition.id);
    const selection = { provider, model: fallbackModels(provider, definition.id)[0].id };
    if (index === "primary") onChange({ ...route, primary: selection });
    else onChange({ ...route, fallbacks: route.fallbacks.map((item, itemIndex) => itemIndex === index ? selection : item) });
  };
  return (
    <article className={`model-route-card ${definition.live ? "is-live" : "is-future"}`}>
      <header><span className="route-icon"><Icon name={definition.icon} /></span><div><h4>{definition.label}</h4><p>{definition.copy}</p></div><b>{definition.live ? "ATTIVO" : "FUTURO"}</b></header>
      <div className="route-label"><span>MODELLO PRINCIPALE</span><i /></div>
      <SelectionRow selection={route.primary} models={modelsFor(route.primary)} providers={providerOptions} onProvider={(provider) => changeProvider("primary", provider)} onModel={(model) => onChange({ ...route, primary: { ...route.primary, model } })} />
      {route.fallbacks.map((selection, index) => <div className="fallback-block" key={`${index}-${selection.provider}`}><div className="route-label"><span>FALLBACK {index + 1} · CONFIGURATO, NON ANCORA ESEGUITO</span><button type="button" onClick={() => onChange({ ...route, fallbacks: route.fallbacks.filter((_, itemIndex) => itemIndex !== index) })}>Rimuovi</button></div><SelectionRow selection={selection} models={modelsFor(selection)} providers={providerOptions} onProvider={(provider) => changeProvider(index, provider)} onModel={(model) => onChange({ ...route, fallbacks: route.fallbacks.map((item, itemIndex) => itemIndex === index ? { ...item, model } : item) })} /></div>)}
      {route.fallbacks.length < 2 && <button className="add-fallback" type="button" onClick={() => { const provider = providerOptions.find((item) => item !== route.primary.provider) ?? providerOptions[0]; onChange({ ...route, fallbacks: [...route.fallbacks, { provider, model: fallbackModels(provider, definition.id)[0].id }] }); }}>+ Aggiungi fallback</button>}
    </article>
  );
}

function SelectionRow({ selection, models, providers, onProvider, onModel }: { selection: ModelSelection; models: ModelOption[]; providers: ProviderId[]; onProvider: (provider: ProviderId) => void; onModel: (model: string) => void }) {
  return <div className="selection-row"><label><span>Provider</span><select value={selection.provider} onChange={(event) => onProvider(event.target.value as ProviderId)}>{providers.map((provider) => <option value={provider} key={provider}>{providerLabel(provider)}</option>)}</select></label><label><span>Modello</span><select value={selection.model} onFocus={() => undefined} onChange={(event) => onModel(event.target.value)}>{models.map((model) => <option value={model.id} key={model.id}>{model.name}</option>)}</select></label></div>;
}

function ChatGptCodexConnection({ onConnection, setNotice }: {
  onConnection: (connected: boolean) => void;
  setNotice: (notice: Notice | null) => void;
}) {
  const [status, setStatus] = useState<CodexAccountStatus | null>(null);
  const [limits, setLimits] = useState<CodexRateLimits | null>(null);
  const [deviceLogin, setDeviceLogin] = useState<CodexDeviceLoginStart | null>(null);
  const [busy, setBusy] = useState(false);
  const [polling, setPolling] = useState(false);
  const [copied, setCopied] = useState(false);
  const pollTimer = useRef<number | undefined>(undefined);
  const pollAttempts = useRef(0);
  const mounted = useRef(true);
  const onConnectionRef = useRef(onConnection);
  const lastReportedConnection = useRef<boolean | null>(null);
  onConnectionRef.current = onConnection;

  const stopPolling = useCallback(() => {
    if (pollTimer.current) window.clearTimeout(pollTimer.current);
    pollTimer.current = undefined;
    setPolling(false);
  }, []);

  const refresh = useCallback(async (silent = false) => {
    try {
      const next = await chatgptAccountStatus();
      if (!mounted.current) return next;
      setStatus(next);
      if (lastReportedConnection.current !== next.connected) {
        lastReportedConnection.current = next.connected;
        onConnectionRef.current(next.connected);
      }
      if (next.connected) {
        stopPolling();
        setDeviceLogin(null);
        const nextLimits = await chatgptRateLimits().catch(() => ({}));
        if (mounted.current) setLimits(nextLimits);
      }
      return next;
    } catch (cause) {
      if (mounted.current) {
        setStatus({ available: false, connected: false });
        if (lastReportedConnection.current !== false) {
          lastReportedConnection.current = false;
          onConnectionRef.current(false);
        }
        if (!silent) setNotice({ kind: "error", text: errorMessage(cause) });
      }
      return null;
    }
  }, [setNotice, stopPolling]);

  const beginPolling = useCallback(() => {
    stopPolling();
    pollAttempts.current = 0;
    setPolling(true);
    const tick = async () => {
      const next = await refresh(true);
      if (!mounted.current || next?.connected) return;
      pollAttempts.current += 1;
      if (pollAttempts.current >= 90) {
        setPolling(false);
        setNotice({ kind: "info", text: "Login ChatGPT ancora non rilevato. Puoi riprovare o usare il codice dispositivo." });
        return;
      }
      pollTimer.current = window.setTimeout(() => void tick(), 2_000);
    };
    void tick();
  }, [refresh, setNotice, stopPolling]);

  useEffect(() => {
    mounted.current = true;
    void refresh(true);
    return () => {
      mounted.current = false;
      if (pollTimer.current) window.clearTimeout(pollTimer.current);
    };
  }, [refresh]);

  async function loginBrowser() {
    setBusy(true);
    try {
      await beginChatgptLogin();
      setNotice({ kind: "info", text: "Completa il login ChatGPT nel browser. Onyx rileverà automaticamente l’account." });
      beginPolling();
    } catch (cause) {
      setNotice({ kind: "error", text: errorMessage(cause) });
    } finally {
      setBusy(false);
    }
  }

  async function loginDevice() {
    setBusy(true);
    try {
      const login = await beginChatgptDeviceLogin();
      setDeviceLogin(login);
      setNotice({ kind: "info", text: "Inserisci il codice mostrato nella pagina ChatGPT appena aperta." });
      beginPolling();
    } catch (cause) {
      setNotice({ kind: "error", text: errorMessage(cause) });
    } finally {
      setBusy(false);
    }
  }

  async function copyDeviceCode() {
    if (!deviceLogin) return;
    try {
      await navigator.clipboard.writeText(deviceLogin.userCode);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1_500);
    } catch {
      setNotice({ kind: "error", text: "Non riesco a copiare il codice. Selezionalo manualmente." });
    }
  }

  async function logout() {
    setBusy(true);
    try {
      await disconnectChatgpt();
      stopPolling();
      setDeviceLogin(null);
      setLimits(null);
      const disconnected = { available: true, connected: false };
      setStatus(disconnected);
      lastReportedConnection.current = false;
      onConnectionRef.current(false);
      setNotice({ kind: "ok", text: "Account ChatGPT/Codex disconnesso." });
    } catch (cause) {
      setNotice({ kind: "error", text: errorMessage(cause) });
    } finally {
      setBusy(false);
    }
  }

  const connected = Boolean(status?.connected);
  return (
    <div className={`provider-card provider-card--codex ${connected ? "is-connected" : ""}`}>
      <header>
        <span className="codex-provider-mark">GPT</span>
        <div><strong>ChatGPT / Codex</strong><small>{connected ? status?.email || "Account ChatGPT collegato" : "Usa i modelli inclusi nella subscription Codex"}</small></div>
        <b>{status === null ? "VERIFICA" : !status.available ? "NON DISPONIBILE" : connected ? (status.planType || "CONNESSO").toUpperCase() : "OAUTH"}</b>
      </header>

      {status && !status.available && <p className="codex-unavailable">Il runtime Codex ufficiale non è disponibile. Installa o includi Codex app-server nella build desktop.</p>}

      {status?.available && !connected && <>
        <div className="codex-login-actions">
          <button type="button" className="codex-primary" disabled={busy} onClick={() => void loginBrowser()}>{busy ? "Attendi…" : "Accedi con ChatGPT"}</button>
          <button type="button" disabled={busy} onClick={() => void loginDevice()}>Usa codice dispositivo</button>
        </div>
        {polling && <p className="codex-polling"><i />Attendo il completamento del login…</p>}
        {deviceLogin && <div className="device-code-panel"><span><small>CODICE DISPOSITIVO</small><strong>{deviceLogin.userCode}</strong></span><button type="button" onClick={() => void copyDeviceCode()}>{copied ? "Copiato" : "Copia"}</button><button type="button" onClick={() => void openExternal(deviceLogin.verificationUrl)}>Apri pagina ↗</button></div>}
      </>}

      {connected && <>
        <div className="codex-account-row"><span><b>{status?.email || "ChatGPT"}</b><small>Autenticazione: {status?.authMode || "chatgpt"}</small></span><button type="button" disabled={busy} onClick={() => void refresh(false)}>Aggiorna</button><button type="button" className="codex-logout" disabled={busy} onClick={() => void logout()}>Disconnetti</button></div>
        {limits && <div className="codex-limits">
          <QuotaMeter label="Finestra primaria" used={limits.primaryUsedPercent} minutes={limits.primaryWindowMinutes} resetsAt={limits.primaryResetsAt} />
          <QuotaMeter label="Finestra secondaria" used={limits.secondaryUsedPercent} minutes={limits.secondaryWindowMinutes} resetsAt={limits.secondaryResetsAt} />
        </div>}
      </>}
      <p className="codex-scope-note">Valido per agente e ricerca compatibili con Codex. Non finanzia TTS, trascrizione o API OpenAI generiche.</p>
    </div>
  );
}

function QuotaMeter({ label, used, minutes, resetsAt }: { label: string; used?: number | null; minutes?: number | null; resetsAt?: number | null }) {
  const bounded = Math.min(100, Math.max(0, used ?? 0));
  return <div className="quota-meter"><div><span>{label} · {formatWindow(minutes)}</span><b>{used == null ? "—" : `${Math.round(bounded)}% usato`}</b></div><i><b style={{ width: `${bounded}%` }} /></i><small>{resetsAt ? `Reset ${formatResetTime(resetsAt)}` : "Reset non disponibile"}</small></div>;
}

function formatWindow(minutes?: number | null): string {
  if (!minutes) return "quota";
  if (minutes >= 1_440 && minutes % 1_440 === 0) return `${minutes / 1_440} g`;
  if (minutes >= 60 && minutes % 60 === 0) return `${minutes / 60} h`;
  return `${minutes} min`;
}

function formatResetTime(epochSeconds: number): string {
  try { return new Intl.DateTimeFormat("it-IT", { day: "2-digit", month: "short", hour: "2-digit", minute: "2-digit" }).format(new Date(epochSeconds * 1_000)); }
  catch { return "presto"; }
}

function ProviderConnection({ provider, connected, onConnection, setNotice }: { provider: "openrouter" | "openai" | "anthropic_api"; connected: boolean; onConnection: (connected: boolean) => void; setNotice: (notice: Notice | null) => void }) {
  const [key, setKey] = useState("");
  const [busy, setBusy] = useState(false);
  const meta = provider === "openrouter" ? { icon: "OR", copy: "Più provider con una sola chiave", placeholder: "sk-or-v1-…" } : provider === "openai" ? { icon: "AI", copy: "Responses, ricerca e trascrizione", placeholder: "sk-proj-…" } : { icon: "A", copy: "Modelli Claude via API Anthropic", placeholder: "sk-ant-…" };
  async function save() {
    if (!key.trim()) return;
    setBusy(true);
    try {
      await saveProviderApiKey(provider, key.trim());
      onConnection(true);
      setKey("");
      setNotice({ kind: "ok", text: `${providerLabel(provider)} collegato.` });
    } catch (cause) { setNotice({ kind: "error", text: errorMessage(cause) }); }
    finally { setBusy(false); }
  }
  async function disconnect() {
    try { await disconnectProvider(provider); onConnection(false); setNotice({ kind: "ok", text: `${providerLabel(provider)} disconnesso.` }); }
    catch (cause) { setNotice({ kind: "error", text: errorMessage(cause) }); }
  }
  async function oauth() {
    try { await beginOpenRouterOAuth(); setNotice({ kind: "info", text: "Completa l’accesso OpenRouter nel browser." }); }
    catch (cause) { setNotice({ kind: "error", text: errorMessage(cause) }); }
  }
  return <div className={`provider-card ${connected ? "is-connected" : ""}`}><header><span>{meta.icon}</span><div><strong>{providerLabel(provider)}</strong><small>{meta.copy}</small></div><b>{connected ? "CONNESSO" : "BYOK"}</b></header>{connected ? <button className="disconnect-button" type="button" onClick={() => void disconnect()}>Disconnetti</button> : <><div className="provider-key"><input type="password" value={key} placeholder={meta.placeholder} onChange={(event) => setKey(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter") void save(); }} /><button type="button" disabled={!key.trim() || busy} onClick={() => void save()}>{busy ? "…" : "Salva"}</button></div>{provider === "openrouter" && isTauri && <button className="oauth-link" type="button" onClick={() => void oauth()}>oppure collega con OAuth ↗</button>}</>}</div>;
}

function InfoProvider({ name, label, icon, copy }: { name: string; label: string; icon: IconName; copy: string }) {
  return <div className="provider-card info-provider"><header><span><Icon name={icon} /></span><div><strong>{name}</strong><small>{copy}</small></div><b>{label}</b></header></div>;
}

function BillingPanel({ setNotice }: { setNotice: (notice: Notice | null) => void }) {
  async function checkout() {
    if (!BILLING_CHECKOUT_URL) { setNotice({ kind: "info", text: "Il checkout non è ancora configurato. Imposta VITE_BILLING_CHECKOUT_URL sul tuo checkout server-side." }); return; }
    try { await openExternal(BILLING_CHECKOUT_URL); }
    catch (cause) { setNotice({ kind: "error", text: errorMessage(cause) }); }
  }
  return <div className="billing-layout"><section className="billing-hero"><SkyBackdrop /><div className="billing-copy"><span>ONYX MANAGED</span><h2>Tutto pronto.<br />Nessuna chiave API.</h2><p>Ricerca e trascrizione gestite da Onyx con limiti trasparenti. Il backend di consumo resta separato dall’app desktop.</p><div className="price"><strong>€15</strong><span>/ mese<br /><small>IVA inclusa dove applicabile</small></span></div><button type="button" onClick={() => void checkout()}>Attiva Onyx Managed <span>→</span></button><small>Nessun checkout è simulato: il pulsante funziona solo quando il backend è configurato.</small></div><div className="billing-orb"><i /><i /></div></section><section className="plan-compare"><PlanCard icon="cpu" title="Locale" price="Gratis" items={["I tuoi modelli", "Dati sul dispositivo", "Setup manuale"]} /><PlanCard icon="key" title="BYOK" price="A consumo" items={["OpenRouter / OpenAI", "Chiavi nel portachiavi", "Paghi il provider"]} /><PlanCard icon="spark" title="Managed" price="€15/mese" featured items={["Modelli scelti da Onyx", "Nessuna chiave", "Budget e utilizzo visibili"]} /></section></div>;
}

function VoicePicker({ value, onChange, dark = false }: { value: string; onChange: (value: string) => void; dark?: boolean }) {
  return <div className={`voice-picker ${dark ? "is-dark" : ""}`}>{VOICES.map((voice) => <button key={voice.id} type="button" className={value === voice.id ? "is-selected" : ""} onClick={() => onChange(voice.id)}><span className="voice-thumb"><i /><i /></span><span><b>{voice.name}</b><small>{voice.copy}</small></span>{value === voice.id && <strong>✓</strong>}</button>)}</div>;
}

function ShortcutDemo({ mode }: { mode: "agent" | "dictation" }) {
  return <div className="shortcut-demo"><div><kbd>Ctrl</kbd><span>+</span><kbd>{mode === "agent" ? "Alt" : "Shift"}</kbd></div><p>Tieni premuti entrambi i tasti mentre parli</p></div>;
}

function IslandDemo({ mode }: { mode: "agent" | "dictation" }) {
  return <div className={`island-demo island-demo--${mode}`}><span className="agent-orb"><i /><i /></span><div><strong>{mode === "agent" ? "Ti ascolto" : "Dettatura"}</strong><span>{[5, 9, 17, 11, 22, 14, 7, 12, 5].map((height, index) => <i key={index} style={{ height }} />)}</span></div></div>;
}

function SkyScene({ variant }: { variant: "auth" }) {
  return <section className={`sky-scene sky-scene--${variant}`}><SkyBackdrop /><div className="sky-orb-main"><div className="demo-window"><span className="demo-app">C</span><div><i /><i /><i /><i /><i /><i /><i /></div></div><span className="demo-agent"><i /><div><small>Onyx</small><strong>Come posso aiutarti?</strong></div></span></div><div className="sky-caption"><span>VOICE-FIRST PRODUCTIVITY</span><h2>Dì quello che vuoi.<br />Onyx fa il resto.</h2></div><div className="visual-noise" /></section>;
}

function SkyBackdrop() { return <div className="sky-backdrop"><i /><i /><i /><i /></div>; }
function RouteMini({ icon, label, model }: { icon: IconName; label: string; model: string }) { return <div><span><Icon name={icon} /></span><p><small>{label}</small><strong>{model}</strong></p><i>→</i></div>; }
function ProgressSegment({ label, value, active }: { label: string; value: number; active: boolean }) { return <div className={`progress-segment ${active ? "is-active" : ""}`}><span>{label}</span><i><b style={{ transform: `scaleX(${value})` }} /></i></div>; }
function OnyxBrand({ dark = false, compact = false }: { dark?: boolean; compact?: boolean }) { return <div className={`onyx-brand ${dark ? "is-dark" : ""} ${compact ? "is-compact" : ""}`}><OnyxMark /><strong>ONYX</strong></div>; }
function OnyxMark() { return <svg viewBox="0 0 32 32" aria-hidden="true"><path d="M8.4 6.5c3.7-3 8.9-3.4 13-.8 4.3 2.7 6.5 7.9 5.2 12.8-1.1 4.3-4.8 7.7-9.2 8.4-4.7.8-9.6-1.6-11.7-5.9-2.2-4.6-1.1-10.4 2.7-13.7Z" /><path d="M11.2 10.2c2.3-1.8 5.7-2 8.1-.3 2.6 1.8 3.7 5.2 2.7 8.1-1 3.1-4.2 5-7.4 4.4-3.2-.6-5.5-3.5-5.3-6.7.1-2.1.7-4.1 1.9-5.5Z" /></svg>; }
function GoogleMark() { return <svg viewBox="0 0 24 24" aria-hidden="true"><path fill="#4285f4" d="M21.6 12.2c0-.7-.1-1.4-.2-2H12v3.9h5.4a4.6 4.6 0 0 1-2 3v2.5h3.2c1.9-1.8 3-4.3 3-7.4Z"/><path fill="#34a853" d="M12 22c2.7 0 5-.9 6.6-2.4l-3.2-2.5c-.9.6-2 1-3.4 1-2.6 0-4.8-1.8-5.6-4.2H3.1v2.6A10 10 0 0 0 12 22Z"/><path fill="#fbbc05" d="M6.4 13.9a6 6 0 0 1 0-3.8V7.5H3.1a10 10 0 0 0 0 9l3.3-2.6Z"/><path fill="#ea4335" d="M12 5.9c1.5 0 2.8.5 3.8 1.5l2.9-2.8A9.7 9.7 0 0 0 3.1 7.5l3.3 2.6C7.2 7.7 9.4 5.9 12 5.9Z"/></svg>; }
function AppleMark() { return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M17.1 12.7c0-2.5 2.1-3.7 2.2-3.8a4.7 4.7 0 0 0-3.7-2c-1.6-.2-3.1.9-3.9.9-.8 0-2-1-3.3-1-1.7 0-3.3 1-4.2 2.5-1.8 3.1-.5 7.8 1.3 10.3.9 1.2 1.9 2.6 3.2 2.5 1.3-.1 1.8-.8 3.4-.8 1.6 0 2 .8 3.4.8 1.4 0 2.3-1.3 3.1-2.5 1-1.5 1.5-2.9 1.5-3a4.5 4.5 0 0 1-3-3.9ZM14.6 5.3c.7-.9 1.2-2.1 1.1-3.3-1.1 0-2.4.7-3.2 1.6-.7.8-1.3 2-1.1 3.2 1.2.1 2.5-.6 3.2-1.5Z" /></svg>; }

function StatCard({ label, value, copy, icon }: { label: string; value: string; copy: string; icon: IconName }) { return <article className="dashboard-card stat-card"><span className="blue-icon"><Icon name={icon} /></span><div><small>{label}</small><strong>{value}</strong><p>{copy}</p></div></article>; }
function QuickItem({ done = false, label, copy }: { done?: boolean; label: string; copy: string }) { return <div><span className={done ? "is-done" : ""}>{done ? "✓" : "○"}</span><p><strong>{label}</strong><small>{copy}</small></p></div>; }
function CardTitle({ icon, title, copy }: { icon: IconName; title: string; copy: string }) { return <div className="card-title"><span className="blue-icon"><Icon name={icon} /></span><div><h3>{title}</h3><p>{copy}</p></div></div>; }
function EmptyState({ icon, title, copy }: { icon: IconName; title: string; copy: string }) { return <div className="empty-state"><span><Icon name={icon} /></span><h4>{title}</h4><p>{copy}</p></div>; }
function PlanCard({ icon, title, price, items, featured = false }: { icon: IconName; title: string; price: string; items: string[]; featured?: boolean }) { return <article className={featured ? "is-featured" : ""}><header><span><Icon name={icon} /></span><div><h3>{title}</h3><b>{price}</b></div></header>{items.map((item) => <p key={item}><i>✓</i>{item}</p>)}</article>; }

function providerOptionsFor(capability: CapabilityId): ProviderId[] {
  if (capability === "stt") return ["openrouter", "openai"];
  if (capability === "web_search") return ["openrouter", "openai", "chatgpt_codex"];
  if (capability === "tts") return ["local", "openai", "openrouter", "managed"];
  if (capability === "images") return ["openai", "openrouter", "local", "managed"];
  if (capability === "video") return ["openrouter", "local", "managed"];
  return PROVIDERS.filter((provider) => provider !== "chatgpt_codex");
}
function formatDate(value: string) { try { return new Intl.DateTimeFormat("it-IT", { dateStyle: "medium", timeStyle: "short" }).format(new Date(value)); } catch { return value; } }
function initials(profile: OnyxProfile) { return `${profile.firstName[0] ?? "O"}${profile.lastName[0] ?? ""}`.toUpperCase(); }
function fullName(profile: OnyxProfile) { return `${profile.firstName} ${profile.lastName}`.trim(); }

type IconName = "home" | "history" | "mic" | "spark" | "route" | "card" | "globe" | "monitor" | "file" | "volume" | "image" | "video" | "key" | "cpu" | "lock" | "access" | "play" | "sliders" | "wave" | "brain";
function Icon({ name }: { name: IconName }) {
  const common = { fill: "none", stroke: "currentColor", strokeWidth: 1.8, strokeLinecap: "round" as const, strokeLinejoin: "round" as const };
  const paths: Record<IconName, ReactNode> = {
    home: <><path d="m3 11 9-8 9 8"/><path d="M5 10v10h14V10M9 20v-6h6v6"/></>,
    history: <><path d="M3 12a9 9 0 1 0 3-6.7L3 8"/><path d="M3 3v5h5M12 7v5l3 2"/></>,
    mic: <><rect x="8" y="3" width="8" height="13" rx="4"/><path d="M5 11a7 7 0 0 0 14 0M12 18v3M9 21h6"/></>,
    spark: <><path d="m12 2 1.5 5.3L19 9l-5.5 1.7L12 16l-1.5-5.3L5 9l5.5-1.7L12 2Z"/><path d="m19 15 .7 2.3L22 18l-2.3.7L19 21l-.7-2.3L16 18l2.3-.7L19 15Z"/></>,
    route: <><circle cx="5" cy="5" r="2"/><circle cx="19" cy="19" r="2"/><circle cx="19" cy="5" r="2"/><path d="M7 5h10M5 7v5a7 7 0 0 0 7 7h5"/></>,
    card: <><rect x="3" y="5" width="18" height="14" rx="2"/><path d="M3 10h18M7 15h3"/></>,
    globe: <><circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3a15 15 0 0 1 0 18M12 3a15 15 0 0 0 0 18"/></>,
    monitor: <><rect x="3" y="4" width="18" height="13" rx="2"/><path d="M8 21h8M12 17v4"/></>,
    file: <><path d="M6 2h8l4 4v16H6zM14 2v5h5"/><path d="M9 12h6M9 16h6"/></>,
    volume: <><path d="M4 10v4h4l5 4V6l-5 4H4Z"/><path d="M16 9a4 4 0 0 1 0 6M19 6a8 8 0 0 1 0 12"/></>,
    image: <><rect x="3" y="4" width="18" height="16" rx="2"/><circle cx="9" cy="9" r="2"/><path d="m3 17 5-5 4 4 2-2 7 6"/></>,
    video: <><rect x="3" y="6" width="13" height="12" rx="2"/><path d="m16 10 5-3v10l-5-3"/></>,
    key: <><circle cx="8" cy="15" r="4"/><path d="m11 12 9-9M16 7l2 2M18 5l2 2"/></>,
    cpu: <><rect x="7" y="7" width="10" height="10" rx="2"/><path d="M9 2v3M15 2v3M9 19v3M15 19v3M2 9h3M2 15h3M19 9h3M19 15h3"/></>,
    lock: <><rect x="5" y="10" width="14" height="11" rx="2"/><path d="M8 10V7a4 4 0 0 1 8 0v3M12 14v3"/></>,
    access: <><circle cx="12" cy="4" r="2"/><path d="M4 8h16M12 8v13M8 21l4-7 4 7"/></>,
    play: <><circle cx="12" cy="12" r="9"/><path d="m10 8 6 4-6 4V8Z"/></>,
    sliders: <><path d="M4 6h16M4 12h16M4 18h16"/><circle cx="8" cy="6" r="2"/><circle cx="15" cy="12" r="2"/><circle cx="10" cy="18" r="2"/></>,
    wave: <><path d="M3 12h2l2-6 4 12 3-9 3 6 2-3h2"/></>,
    brain: <><path d="M9 4a3 3 0 0 0-5 2.2A3.5 3.5 0 0 0 4 13a3 3 0 0 0 5 5M15 4a3 3 0 0 1 5 2.2 3.5 3.5 0 0 1 0 6.8 3 3 0 0 1-5 5M9 4v16M15 4v16M9 8H7M15 8h2M9 15H7M15 15h2"/></>,
  };
  return <svg viewBox="0 0 24 24" aria-hidden="true" {...common}>{paths[name]}</svg>;
}
