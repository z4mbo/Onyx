# Build macOS

Onyx viene compilato come applicazione universale per Apple Silicon e Intel dal workflow `Build macOS Universal`.

Il workflow produce:

- `Onyx_0.10.1_macOS_universal.dmg`;
- `Onyx_0.10.1_macOS_universal.app.zip`;
- `SHA256SUMS.txt`.

La build di test usa una firma ad-hoc. Dopo il download macOS può quindi richiedere l'autorizzazione manuale in **Impostazioni di Sistema → Privacy e Sicurezza**. Per una distribuzione pubblica vanno configurati un certificato Developer ID Application e la notarizzazione Apple.

Al primo avvio Onyx richiede Microfono, Monitoraggio input e Accessibilità. Questi permessi servono rispettivamente per registrare, rilevare le combinazioni tenute premute e inserire la trascrizione nell'app attiva.

La build macOS deve essere eseguita su macOS: da Windows non sono disponibili SDK, linker, firma e strumenti DMG Apple.
