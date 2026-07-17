# Architettura Onyx 0.10

## Obiettivo della release

La 0.10 estende la shell desktop voice-first con login ChatGPT/Codex ufficiale e sintesi vocale multi-provider:

- accesso iniziale e onboarding visuale;
- dashboard e impostazioni in stile Onyx/VoiceOS;
- dettatura hold-to-talk nell'app in primo piano;
- agente hold-to-talk limitato alla ricerca web;
- routing per capacità e provider, progettato per aggiungere più modelli senza legare ogni tool a un solo LLM.

La separazione tra capacità e provider è intenzionale: in futuro un utente potrà usare, per esempio, un modello locale per STT, OpenRouter per la ricerca e un modello managed per computer use. La struttura dati lo rappresenta già; la 0.9 esegue solo i percorsi esplicitamente segnati come operativi.

## Finestre Tauri

| Label | Dimensioni | Responsabilità |
|---|---:|---|
| `main` | 1120×740, ridimensionabile | auth/preview, onboarding, dashboard, cronologia, provider e capability routing |
| `hud` | 206×64, trasparente | pill di dettatura in basso, waveform e contesto app; always-on-top e non focalizzabile |
| `agent` | 268×66 → 420×620 | Dynamic Island in alto; registra da collassata, poi si espande per risposta, fonti e utilizzo |

La finestra principale si nasconde nel tray invece di terminare il processo. Gli overlay non entrano nella taskbar. L'HUD non prende il focus, così il testo trascritto può essere reinserito nella finestra originaria.

## Gesti globali

Il listener nativo in `modifier_hold.rs` usa i modificatori di sinistra e richiede una pressione stabile di 180 ms:

```text
Ctrl sinistro + Shift sinistro premuti
  → evento onyx://hold { mode: dictation, phase: pressed }
  → mostra HUD e avvia microfono
rilascio di uno dei due tasti
  → evento ... phase: released
  → termina audio, trascrive e inserisce testo

Ctrl sinistro + Alt sinistro premuti
  → evento onyx://hold { mode: agent, phase: pressed }
  → mostra Dynamic Island e avvia microfono
rilascio di uno dei due tasti
  → trascrizione domanda → ricerca web → risposta/fonti → TTS locale
```

Su Windows il listener usa un low-level keyboard hook e non intercetta/blocca gli eventi inoltrati alle altre app. Su macOS controlla lo stato delle keycode dei modificatori. Un altro tasto non modificatore durante la fase candidata annulla l'attivazione, riducendo attivazioni involontarie mentre si usano normali scorciatoie.

## Flusso dettatura

```text
hold Ctrl+Shift
  → acquisizione contesto app in primo piano
  → getUserMedia + livello RMS
  → waveform reattiva
release
  → WAV PCM mono 16 kHz
  → provider STT selezionato
  → testo
  → input nativo Enigo nella finestra che mantiene il focus
  → HUD nascosto
```

Percorsi STT attivi:

- OpenRouter: `POST /api/v1/audio/transcriptions` con audio base64 nel payload;
- OpenAI: `POST /v1/audio/transcriptions` multipart.

Non vengono eseguiti retry automatici del POST, per evitare doppie richieste fatturabili. Il backend valida dimensione/formato dell'audio, modello e forma della chiave prima dell'invio.

`active_app.rs` riconosce il processo foreground su Windows e restituisce nome, simbolo e accent color per app comuni. È un badge semantico, non l'icona originale del file eseguibile. Su macOS il contesto applicazione è ancora un fallback generico.

## Flusso agente di ricerca

```text
hold Ctrl+Alt
  → Dynamic Island collassata + microfono
release
  → STT configurato
  → SearchRequest { query, provider, model, reasoning }
  ├─ OpenRouter Chat Completions + plugin web
  ├─ OpenAI Responses API + tool web_search
  └─ Codex app-server + account ChatGPT e web search
  → SearchReply { answer, model, sources[], usage }
  → overlay espanso
  → TTS nativo, OpenAI o OpenRouter opzionale
```

Le fonti vengono estratte dalle annotation URL del provider, deduplicate e mostrate come link apribili solo se `http`/`https`. Onyx visualizza i token quando il provider li restituisce e il costo quando disponibile. Non esistono tool loop, shell, browser automation, accesso ai file o computer control in questa release.

Il TTS può usare la voce nativa Windows/macOS oppure gli endpoint Audio Speech di OpenAI e OpenRouter. La configurazione persistente contiene provider, modello, voce e velocità ma mai la chiave, che resta nel keyring. Se una sintesi cloud fallisce, Onyx può ripiegare sulla voce di sistema.

## Capability router

`CapabilityRoute` separa la funzione dal modello:

```ts
type CapabilityRoute = {
  capability: "stt" | "web_search" | "computer" | "files" | "tts" | "images" | "video";
  primary: { provider: ProviderId; model: string };
  fallbacks: Array<{ provider: ProviderId; model: string }>;
};
```

| Capacità | Runtime 0.9 | Provider selezionabili/predisposti |
|---|---|---|
| `stt` | Operativo | OpenRouter, OpenAI; local/managed sono placeholder |
| `web_search` | Operativo | OpenRouter, OpenAI; local/managed/Anthropic/Claude SDK pianificati |
| `tts` | Operativo solo come voce di sistema | local system voice; OpenAI TTS è solo catalogo futuro |
| `computer` | Non eseguito | routing futuro local/managed |
| `files` | Non eseguito | routing futuro local/managed |
| `images` | Non eseguito | catalogo futuro OpenAI/OpenRouter/managed |
| `video` | Non eseguito | catalogo futuro managed |

Il frontend conserva un modello primario e fino a tre fallback per capacità. Nella 0.9 il runtime usa il modello primario delle rotte attive e **non esegue failover automatico**. Prima di abilitarlo serviranno policy esplicite per errori ritentabili, limiti di spesa, consenso al cambio provider e deduplicazione delle richieste fatturabili.

## Provider e credenziali

### Operativi

- **OpenRouter BYOK:** API key verificata prima del salvataggio, catalogo dinamico, OAuth OpenRouter PKCE compatibile con la 0.8, STT e web search.
- **OpenAI BYOK:** API key verificata tramite catalogo modelli, STT, Responses API con `web_search` e TTS.
- **ChatGPT/Codex:** `codex app-server` gestisce OAuth, token, catalogo modelli, ricerca e quota della subscription; Onyx non legge i token.

Le credenziali sono salvate tramite il crate `keyring` nel Windows Credential Manager o nel Portachiavi macOS, sotto il service `com.onyx.assistant`. Non vengono restituite al frontend e non sono persistite in `localStorage`.

### Predisposti ma non eseguibili end-to-end

- **Local:** sono presenti route e model placeholder, ma nessun server OpenAI-compatible, Whisper runtime, motore di ricerca o pesi è incluso/configurato.
- **Managed:** il catalogo descrive modelli inclusi nel piano, ma non esiste ancora un backend Onyx che autentichi e inoltri le richieste.
- **Anthropic API:** validazione key e catalogo modelli sono predisposti; la ricerca restituisce intenzionalmente “fase successiva”.
- **Claude subscription / Agent SDK:** rappresentato come capability futura; non esistono login, token exchange o processo SDK nella build.

Un account ChatGPT/Claude con abbonamento consumer non viene trattato come una API key generica. La build non estrae cookie o token dalle sessioni web.

## Auth Clerk

### Stato 0.9

Il frontend distingue `authMode: preview | clerk`:

- `preview` salva un profilo locale sufficiente a provare onboarding e interfaccia;
- se `VITE_CLERK_SIGN_IN_URL` è valorizzato, il pulsante social apre la pagina Clerk hosted nel browser di sistema.

Questo non costituisce ancora una sessione desktop verificata: manca il callback che riconsegna l'identità all'app.

### Architettura richiesta per produzione

```text
Onyx desktop (public client)
  → browser di sistema → Clerk hosted sign-in / Google OAuth
  → Authorization Code + PKCE
  → callback loopback 127.0.0.1:<porta-casuale>
  → verifica state/nonce + completamento sessione
  → token rinnovabile nel portachiavi
  → backend Onyx verifica il token Clerk a ogni richiesta managed
```

La Clerk application deve abilitare i provider social desiderati e registrare i redirect ammessi. Nessun `client_secret`, signing key o secret Stripe deve essere compilato in Vite/Tauri. Il callback loopback va aperto prima di lanciare il browser, associato a una singola richiesta e chiuso dopo successo/timeout.

## Billing Managed

Il prezzo prodotto desiderato è €15/mese, ma Stripe non è incluso in questa repository. Il confine corretto è:

```text
Onyx → backend autenticato → Stripe Checkout
Stripe webhook → backend → entitlement Clerk/customer
Onyx → backend managed → quota/rate limit → provider AI
```

Il client non decide autonomamente se il pagamento è valido e non possiede le chiavi AI gestite. L'accesso al piano deve dipendere da entitlement server-side derivato da webhook Stripe verificato, con idempotenza, scadenza e gestione di cancellazioni/rimborsi.

## Persistenza e privacy

| Dato | Posizione | Note |
|---|---|---|
| API key provider | Keychain nativo | non leggibile dal frontend |
| preferenze e route | `localStorage` webview | non contengono secret |
| profilo preview/onboarding | `localStorage` webview | solo stato locale, non identità verificata |
| cronologia ricerca | `localStorage` webview | massimo 100 elementi; cancellabile |
| audio | memoria durante il flusso | inviato al provider STT selezionato al rilascio |
| testo dettato | memoria transitoria | inserito nell'app target; non aggiunto alla cronologia ricerca |

L'uso di OpenRouter/OpenAI implica l'invio dell'audio e/o della domanda al provider scelto. Il TTS di sistema non richiede un upload da parte di Onyx, salvo comportamento proprio della voce installata/OS che va documentato separatamente per piattaforma.

## Moduli principali

- `src/types.ts`: settings, provider, capability routes, search result e contesto app.
- `src/lib/storage.ts`: migrazioni e persistenza locale non sensibile.
- `src/lib/audio.ts`: acquisizione, downmix, resampling e WAV PCM16.
- `src/lib/api.ts`: bridge IPC e fallback di anteprima browser.
- `src-tauri/src/modifier_hold.rs`: state machine dei gesti modifier-only.
- `src-tauri/src/active_app.rs`: contesto dell'app foreground.
- `src-tauri/src/provider.rs`: cataloghi, STT e web search OpenRouter/OpenAI.
- `src-tauri/src/secrets.rs`: credenziali provider nel keychain.
- `src-tauri/src/windowing.rs`: dimensionamento/posizionamento degli overlay.
- `src-tauri/src/commands.rs`: superficie IPC validata.

## Limiti noti prima di una beta

- completare sessione Clerk desktop e verifica backend;
- implementare checkout, webhook ed entitlement Stripe;
- definire endpoint e schema per provider locali;
- collegare Anthropic/Claude soltanto tramite flussi ufficiali compatibili;
- aggiungere failover consapevole dei costi;
- estrarre vere icone foreground e completare l'equivalente macOS;
- aggiungere streaming di trascrizione/risposta e cancellazione richieste;
- firmare/notarizzare gli installer per ridurre gli avvisi di Smart App Control e Gatekeeper.
