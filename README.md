# Onyx 0.10

Onyx è un'app desktop voice-first per Windows e macOS, scritta in Rust, Tauri e React. La 0.10 aggiunge login ChatGPT/Codex con quota della subscription, catalogo modelli reale e voci selezionabili tramite sistema, OpenAI o OpenRouter.

> Stato: alpha testabile. Dettatura, ricerca web, login ChatGPT/Codex e TTS selezionabile sono operativi; login Clerk e piano Managed richiedono ancora servizi esterni.

## Cosa funziona oggi

- **Dettatura:** tieni premuti `Ctrl sinistro + Shift sinistro`, parla e rilascia. Onyx trascrive e inserisce il testo nell'app che aveva il focus.
- **Ricerca vocale:** tieni premuti `Ctrl sinistro + Alt sinistro`, formula una domanda e rilascia. L'overlay in alto mostra risposta, fonti, modello e utilizzo disponibile.
- **BYOK:** OpenRouter e OpenAI supportano modelli distinti per trascrizione e ricerca. OpenRouter può essere collegato anche con il suo OAuth PKCE; OpenAI usa una API key.
- **ChatGPT subscription:** Onyx usa il runtime ufficiale `codex app-server`; OAuth, token e rinnovo restano sotto la gestione di Codex. Il dropdown mostra i modelli realmente disponibili all'account.
- **Risposta vocale:** puoi scegliere voce di sistema, OpenAI Audio Speech o i modelli TTS pubblicati da OpenRouter. Modello, voce e velocità scelti vengono inviati realmente al provider.
- **Contesto visivo:** durante la dettatura l'HUD mostra un badge e un colore ricavati dall'app in primo piano. Su Windows vengono riconosciute diverse app comuni; non viene ancora estratta l'icona originale dell'eseguibile.
- **Percorsi per capacità:** la configurazione separa STT, ricerca, TTS, computer, file, immagini e video, con un modello primario e fallback memorizzabili. Nella 0.9 vengono eseguiti soltanto STT e ricerca web; il failover automatico non è ancora attivo.
- **Dati locali:** profilo di anteprima, onboarding, preferenze, routing e cronologia di ricerca restano sul dispositivo. Le API key vengono salvate nel portachiavi nativo.

L'agente 0.9 **non controlla il computer, non apre app e non legge o modifica file**. Questa scelta mantiene la prima iterazione limitata alla ricerca web con fonti visibili.

## Primo avvio

1. Avvia Onyx e completa l'accesso di anteprima oppure apri il login Clerk configurato.
2. Segui l'onboarding per lingua, microfono, voce, gesti e provider.
3. In **Modelli**, collega OpenRouter o OpenAI e scegli separatamente il modello STT e quello di ricerca.
4. Prova i due gesti hold-to-talk. La combinazione deve rimanere premuta per circa 180 ms e termina al rilascio.

L'inserimento nativo su macOS richiede l'autorizzazione **Privacy e sicurezza → Accessibilità**. Su Windows Onyx non può scrivere in una finestra eseguita come amministratore se Onyx non ha lo stesso livello di privilegi.

## Matrice provider

| Percorso | Stato nella 0.10 | Note |
|---|---|---|
| OpenRouter BYOK | Operativo per STT e ricerca | API key o OAuth OpenRouter; ricerca via plugin web con citazioni |
| OpenAI BYOK | Operativo per STT e ricerca | API key; ricerca via Responses API e tool `web_search` |
| ChatGPT / Codex | Operativo per l'agente | OAuth ufficiale Codex, modelli e quota della subscription |
| TTS locale | Operativo | Voce nativa Windows/macOS, senza costo API |
| OpenAI / OpenRouter TTS | Operativo | Modello, voce e velocità selezionabili; consumo a carico della API key scelta |
| Modelli locali | Configurazione predisposta | Nessun runtime, endpoint o peso AI locale è incluso in questa build |
| Anthropic API | Key validation e catalogo predisposti | Esecuzione della ricerca web non ancora collegata |
| Claude Agent SDK / account | Solo percorso pianificato | Nessun login Claude o runtime subscription è incluso |
| Onyx Managed | UI e catalogo dimostrativo | Backend, proxy provider, quote ed entitlement non inclusi |
| Computer, file, immagini, video | Routing predisposto | Nessun tool di esecuzione nella 0.9 |

Il login ChatGPT usa esclusivamente il percorso ufficiale Codex e la relativa quota della subscription. Non trasforma l'abbonamento in credito API generico: STT, TTS, immagini e provider esterni richiedono ancora una route locale, BYOK o Managed. Onyx non legge cookie del browser e non copia token di sessione.

## Clerk: configurazione attuale e produzione

La UI corrente supporta due percorsi:

- senza configurazione, un **accesso di anteprima locale** permette di testare onboarding e dashboard;
- con `VITE_CLERK_SIGN_IN_URL`, il pulsante social apre nel browser la pagina di accesso ospitata da Clerk.

Copia `.env.example` in `.env.local` e imposta l'URL pubblico della tua istanza Clerk:

```dotenv
VITE_CLERK_SIGN_IN_URL=https://your-instance.clerk.accounts.dev/sign-in
```

L'apertura dell'URL, da sola, non crea ancora una sessione verificata dentro Onyx. Per la produzione serve una Clerk application pubblica con i provider social abilitati e un flusso desktop Authorization Code + PKCE: browser di sistema, `state`/`nonce`, callback su loopback `127.0.0.1` con porta temporanea (o redirect desktop esplicitamente supportato), verifica server-side/sessione Clerk e conservazione sicura del token rinnovabile. Nessun client secret va inserito nel bundle Tauri.

## Piano Onyx Managed da €15/mese

La schermata del piano descrive il prezzo desiderato, ma questa repository **non contiene** checkout Stripe, webhook, backend di entitlement o chiavi provider gestite. Per renderlo acquistabile servono almeno:

1. un prodotto/prezzo Stripe ricorrente da €15/mese;
2. un backend che crei Checkout/Customer Portal e verifichi i webhook;
3. il collegamento stabile tra utente Clerk, customer Stripe e stato dell'abbonamento;
4. un proxy autenticato per STT/ricerca che applichi quote e rate limit senza distribuire chiavi master nel client.

La UI non deve considerare un utente abbonato in base al solo redirect di Checkout: l'entitlement deve arrivare dal backend dopo la verifica del webhook.

## Sviluppo

Prerequisiti: Node.js/pnpm, toolchain Rust compatibile con `rust-toolchain.toml` e dipendenze Tauri per il sistema operativo.

```powershell
pnpm install
pnpm tauri dev
```

Verifiche:

```powershell
pnpm test
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml
```

Build desktop:

```powershell
pnpm tauri build
```

La build non include né scarica modelli AI locali. Consulta [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) per finestre, flussi e confini di sicurezza.
