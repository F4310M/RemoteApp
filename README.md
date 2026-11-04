# RemoteApp

Desktop remoto **diretto** tra due PC Windows e Linux: nessun server nel mezzo, il video e l'input viaggiano cifrati da un PC all'altro.

> Stato: **prima versione (MVP) per LAN / IP diretto**. Funziona tra PC della stessa rete, via VPN (WireGuard, Tailscale…) o verso un PC con porta aperta / IPv6. Il collegamento attraverso NAT via Internet arriva più avanti.

## Come si usa

Sul PC da controllare:

```
remoteapp host
```

Mostra indirizzo, **impronta** (fingerprint) e **password**:

```
  Address:     192.168.1.20  (UDP port 47800)
  Fingerprint: 3248-398E-3F27-2BA5-B124-30DD-0A5B-FE0D
  Password:    k7mqv3xhpa
```

Sul PC da cui controllare:

```
remoteapp connect 192.168.1.20 --fingerprint 3248-398E-3F27-2BA5-B124-30DD-0A5B-FE0D
```

Chiede la password e apre una finestra con lo schermo remoto. Mouse e tastiera nella finestra vanno al PC remoto. Chiudi la finestra per disconnetterti.

Senza `--fingerprint` il client mostra l'impronta e chiede conferma (come SSH). Dopo la prima volta se la ricorda.

### Abbinare due PC (niente più password)

Aggiungi `--pair` alla prima connessione:

```
remoteapp connect 192.168.1.20 --fingerprint 3248-398E-... --pair
```

Dopo questa connessione con password, i due PC si riconoscono dalle chiavi:

- il client conosce la chiave pubblica dell'host (impronta confermata);
- l'host salva la chiave pubblica del client nell'elenco dei dispositivi abbinati.

Dalle volte successive basta `remoteapp connect 192.168.1.20`. Le chiavi private non lasciano mai il proprio PC.

Sull'host:

```
remoteapp devices            # elenca i dispositivi abbinati
remoteapp revoke <nome>      # revoca (anche con le prime cifre dell'impronta)
remoteapp host --paired-only # accetta solo dispositivi abbinati, password disattivata
```

Opzioni utili per `host`: `--password <fissa>` (o variabile `REMOTEAPP_PASSWORD`), `--fps 30`, `--bitrate 8000` (kbit/s), `--port 47800`, `--paired-only`.

**Firewall:** sul PC host va consentita in ingresso la porta **UDP 47800** (Windows lo chiede al primo avvio).

## Sicurezza

- Trasporto **QUIC con TLS 1.3**: tutto è cifrato.
- Ogni PC crea al primo avvio la sua chiave (certificato autofirmato) e la usa sia da host sia da client. Le due parti si presentano la chiave a vicenda nella connessione TLS (autenticazione mutua).
- Il client accetta l'host solo se l'impronta corrisponde a quella confermata dall'utente, quindi nessuno può fingersi il tuo PC.
- L'host fa entrare senza password solo le chiavi abbinate. Gli altri devono dare la password; ogni tentativo sbagliato costa 2 secondi e si prova un solo tentativo alla volta. Con `--paired-only` la password è disattivata del tutto.
- Un solo visualizzatore alla volta.

File salvati in `~/.config/RemoteApp` (Linux) o `%APPDATA%\RemoteApp\config` (Windows): `host_cert.der` / `host_key.der` (la chiave del PC, la seconda è privata), `known_hosts` (host conosciuti), `paired_devices` (dispositivi abbinati).

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

Codice in `src/`: `proto.rs` (protocollo), `tls.rs` (identità e cifratura), `keymap.rs`, `host/` (cattura, codifica, input), `client/` (connessione, decodifica, finestra).

## Limiti noti di questa versione

- **Wayland** non supportato come host: su Linux serve una sessione Xorg.
- Windows: si vede solo il **monitor principale**; schermata UAC e schermata di blocco non si vedono (servirà un servizio di sistema).
- Linux: tutto il desktop X11 (tutti i monitor insieme).
- Codifica solo software: con schermi 4K e scene molto animate la CPU lavora parecchio.
- Niente audio, appunti condivisi o trasferimento file.

## Prossimi passi

1. Collegamento via Internet: STUN/ICE e hole punching, codice di connessione.
2. Codifica hardware (NVENC, QuickSync, VAAPI).
3. Wayland (PipeWire + portal, libei).
4. Scelta del monitor, appunti condivisi, audio, interfaccia grafica.
