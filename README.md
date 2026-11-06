# Magic Remote

Desktop remoto **diretto** tra due PC Windows e Linux: nessun server nel mezzo, il video e l'input viaggiano cifrati da un PC all'altro.

> Stato: **prima versione (MVP) per LAN / IP diretto**. Funziona tra PC della stessa rete, via VPN (WireGuard, Tailscale…) o verso un PC con porta aperta / IPv6. Il collegamento attraverso NAT via Internet arriva più avanti.

## Come si usa

### Con la finestra (consigliato)

Fai doppio clic su `magicremote` (su Windows `magicremote.exe`). Si apre la finestra con quattro schede:

- **I miei PC**: i PC che hai già usato, con un pallino verde se sono accesi. "Connetti" apre lo schermo remoto, "Sveglia e connetti" prima li accende. "+ Aggiungi PC" chiede indirizzo, impronta e password (con l'opzione di abbinarlo).
- **Condividi questo PC**: "Avvia condivisione" mostra indirizzo, impronta e password da dare all'altro PC, chi è collegato ora e i PC abbinati (con "Revoca").
- **Risveglio**: controlla se questo PC si può accendere da remoto e, con "Correggi impostazioni", sistema la scheda di rete (chiede i permessi da amministratore).
- **Sicurezza**: password di accesso fissa o casuale, conferma sul PC prima di far entrare un PC non abbinato, indirizzi bloccati per troppi tentativi e blocco dell'app con PIN (vedi [Sicurezza](#sicurezza)).

**Lingue:** italiano, inglese, tedesco, spagnolo, francese, russo, cinese e giapponese. All'avvio usa la lingua del sistema; si cambia dal menu in alto a destra e la scelta viene ricordata.

### Da riga di comando

Sul PC da controllare:

```
magicremote host
```

Mostra indirizzo, **impronta** (fingerprint) e **password**:

```
  Address:     192.168.1.20  (UDP port 47800)
  Fingerprint: 3248-398E-3F27-2BA5-B124-30DD-0A5B-FE0D
  Password:    k7mqv3xhpa
```

Sul PC da cui controllare:

```
magicremote connect 192.168.1.20 --fingerprint 3248-398E-3F27-2BA5-B124-30DD-0A5B-FE0D
```

Chiede la password e apre una finestra con lo schermo remoto. Mouse e tastiera nella finestra vanno al PC remoto. Chiudi la finestra per disconnetterti.

Senza `--fingerprint` il client mostra l'impronta e chiede conferma (come SSH). Dopo la prima volta se la ricorda.

### Abbinare due PC (niente più password)

Aggiungi `--pair` alla prima connessione:

```
magicremote connect 192.168.1.20 --fingerprint 3248-398E-... --pair
```

Dopo questa connessione con password, i due PC si riconoscono dalle chiavi:

- il client conosce la chiave pubblica dell'host (impronta confermata);
- l'host salva la chiave pubblica del client nell'elenco dei dispositivi abbinati.

Dalle volte successive basta `magicremote connect 192.168.1.20`. Le chiavi private non lasciano mai il proprio PC.

Sull'host:

```
magicremote devices            # elenca i dispositivi abbinati
magicremote revoke <nome>      # revoca (anche con le prime cifre dell'impronta)
magicremote host --paired-only # accetta solo dispositivi abbinati, password disattivata
```

Opzioni utili per `host`: `--password <fissa>` (o variabile `MAGICREMOTE_PASSWORD`), `--fps 30`, `--bitrate 8000` (kbit/s), `--port 47800`, `--paired-only`.

**Firewall:** sul PC host va consentita in ingresso la porta **UDP 47800** (Windows lo chiede al primo avvio).

## Accendere il PC da remoto (standby raggiungibile)

Per raggiungere il PC di casa dal lavoro o dalle vacanze senza lasciarlo acceso: invece di spegnerlo lo metti in **sospensione** (1-3 W). La scheda di rete resta in ascolto e sveglia il PC quando riceve il "pacchetto magico" Wake-on-LAN.

**Una volta, sul PC di casa** (come amministratore su Windows, con `sudo` su Linux):

```
magicremote wake-setup --apply
```

Il comando controlla e sistema tutto quello che serve:

- la scheda di rete accetta il pacchetto magico;
- su Windows è attivo l'ARP offload: da sospeso il PC continua a rispondere al router, che così non lo "dimentica";
- il router apre la porta UDP 47800 da solo (UPnP);
- la linea non è dietro CGNAT;
- c'è un indirizzo IPv6 pubblico.

Alla fine dice se il PC è **risvegliabile da Internet**, solo dalla rete locale, oppure cosa manca.

Poi lascia `magicremote host` in esecuzione e metti il PC in sospensione. `magicremote host` tiene aperta la porta sul router e, a ogni connessione, comunica ai client il MAC e gli indirizzi (locale, pubblico, IPv6).

**Dal PC fuori casa** (che si è già collegato almeno una volta):

```
magicremote wake casa --connect
```

Il comando manda il pacchetto magico a tutti gli indirizzi noti, aspetta che il PC risponda e apre lo schermo remoto. `magicremote hosts` elenca i PC conosciuti, e `magicremote connect casa` si collega per nome.

Limiti:

- Serve il **cavo di rete**: quasi nessuna scheda Wi-Fi sveglia un PC sospeso.
- Con il **CGNAT** (alcune linee FWA e mobili) il PC non è raggiungibile da Internet senza un server. `wake-setup` lo segnala.
- **Linux** non ha l'ARP offload: da Internet il risveglio può non riuscire dopo qualche minuto di sospensione. Dalla rete locale funziona sempre.
- Al risveglio Windows mostra la schermata di blocco. Per vederla e inserire il PIN da remoto serve Magic Remote come servizio di sistema, che è il prossimo passo. Fino ad allora funziona se il PC al risveglio non chiede l'accesso.

## Sicurezza

- Trasporto **QUIC con TLS 1.3**: tutto è cifrato.
- Ogni PC crea al primo avvio la sua chiave (certificato autofirmato) e la usa sia da host sia da client. Le due parti si presentano la chiave a vicenda nella connessione TLS (autenticazione mutua).
- Il client accetta l'host solo se l'impronta corrisponde a quella confermata dall'utente, quindi nessuno può fingersi il tuo PC.
- L'host fa entrare senza password solo le chiavi abbinate. Gli altri devono dare la password; ogni tentativo sbagliato costa 2 secondi e si prova un solo tentativo alla volta. Con `--paired-only` la password è disattivata del tutto.
- **Password fissa** (scheda Sicurezza o `magicremote set-password`): al posto di quella casuale a ogni avvio puoi sceglierne una (almeno 8 caratteri, non troppo facile). Viene salvata solo la sua impronta **Argon2id** con sale casuale, mai la password. `magicremote set-password --clear` torna a quella casuale.
- **Blocco dei tentativi**: dopo 5 password sbagliate dallo stesso indirizzo, quell'indirizzo è bloccato per 15 minuti, poi 30, 60… (fino a 24 ore). Se arrivano più di 30 password sbagliate in 10 minuti da indirizzi diversi, l'accesso con password si sospende per tutti per 10 minuti. I PC abbinati entrano sempre. Dalla scheda Sicurezza si vedono gli indirizzi bloccati e si sbloccano.
- **Conferma sul PC** (scheda Sicurezza o `magicremote approve on`): anche con la password giusta, un PC non abbinato entra solo se chi è davanti all'host preme "Consenti" entro 60 secondi. Da riga di comando la domanda compare nel terminale.
- **Blocco dell'app**: un PIN o una password (hash Argon2id) da inserire per aprire Magic Remote, con blocco automatico dopo 5–60 minuti di inattività. Dopo 3 PIN sbagliati bisogna aspettare 30 secondi, poi sempre di più (fino a 15 minuti), anche chiudendo e riaprendo l'app. La condivisione resta attiva mentre l'app è bloccata. Il blocco protegge l'app aperta, non cifra i file sul disco.
- La finestra passa la password al processo di connessione attraverso una pipe (`connect --password-stdin`), non nella riga di comando né nelle variabili d'ambiente.
- Un solo visualizzatore alla volta.

File salvati in `~/.config/Magic Remote` (Linux) o `%APPDATA%\Magic Remote\config` (Windows): `host_cert.der` / `host_key.der` (la chiave del PC, la seconda è privata), `known_hosts` e `hosts.json` (host conosciuti e come svegliarli), `paired_devices` (dispositivi abbinati), `security.json` (impostazioni di sicurezza e hash delle password).

## Compilare

Serve [Rust](https://rustup.rs) stabile. Nessuna libreria di sistema da installare: OpenH264 viene compilato dai sorgenti.

```
cargo build --release
```

L'eseguibile è in `target/release/`. Ogni push su `main` lo compila anche su GitHub Actions per Linux e Windows (scheda *Actions*, sezione *Artifacts*).

Su Linux il visualizzatore usa `libxkbcommon-x11` (già presente in tutti i desktop).

## Com'è fatto

| Parte | Windows | Linux |
|---|---|---|
| Cattura schermo | DXGI Desktop Duplication (GPU) | X11 + MIT-SHM |
| Video | H.264 con OpenH264 (software) | idem |
| Mouse | `SendInput` (via enigo) | XTest (via enigo) |
| Tastiera | `SendInput` con scancode | XTest con keycode |
| Finestra | winit + softbuffer | idem |
| Rete | QUIC (quinn + rustls) | idem |

I tasti viaggiano come **posizione fisica** (codici evdev), così il layout di tastiera dell'host decide il carattere: con layout italiano su entrambi i PC `ò`, `à`, `€` funzionano.

Codice in `src/`: `proto.rs` (protocollo), `tls.rs` (identità e cifratura), `auth.rs` (password, hash e blocco tentativi), `keymap.rs`, `host/` (cattura, codifica, input), `client/` (connessione, decodifica, finestra).

## Limiti noti di questa versione

- **Wayland** non supportato come host: su Linux serve una sessione Xorg.
- Windows: si vede solo il **monitor principale**; schermata UAC e schermata di blocco non si vedono (servirà un servizio di sistema).
- Linux: tutto il desktop X11 (tutti i monitor insieme).
- Codifica solo software: con schermi 4K e scene molto animate la CPU lavora parecchio.
- Niente audio, appunti condivisi o trasferimento file.
- Nella finestra, i dettagli tecnici del controllo risveglio e il registro attività sono ancora in inglese.

## Prossimi passi

1. Servizio di sistema (Windows e Linux): accesso alla schermata di login/PIN, avvio prima del login.
2. Risveglio, livelli 2 e 3: "compagno di risveglio" (un altro dispositivo di casa con Magic Remote) e presa smart.
3. Collegamento via Internet: STUN/ICE e hole punching, codice di connessione.
4. Codifica hardware (NVENC, QuickSync, VAAPI).
5. Wayland (PipeWire + portal, libei).
6. Scelta del monitor, appunti condivisi, audio.
