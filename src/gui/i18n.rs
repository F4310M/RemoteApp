//! Interface languages. Each text has one entry per language, in the order
//! of [`Lang::ALL`]: it, en, de, es, fr, ru, zh, ja.
//! Placeholders like `{name}` are filled in with [`tf`].

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    It,
    En,
    De,
    Es,
    Fr,
    Ru,
    Zh,
    Ja,
}

impl Lang {
    pub const ALL: [Lang; 8] = [
        Lang::It,
        Lang::En,
        Lang::De,
        Lang::Es,
        Lang::Fr,
        Lang::Ru,
        Lang::Zh,
        Lang::Ja,
    ];

    /// Name of the language in itself, for the language menu.
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::It => "Italiano",
            Lang::En => "English",
            Lang::De => "Deutsch",
            Lang::Es => "Español",
            Lang::Fr => "Français",
            Lang::Ru => "Русский",
            Lang::Zh => "中文",
            Lang::Ja => "日本語",
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Lang::It => "it",
            Lang::En => "en",
            Lang::De => "de",
            Lang::Es => "es",
            Lang::Fr => "fr",
            Lang::Ru => "ru",
            Lang::Zh => "zh",
            Lang::Ja => "ja",
        }
    }

    pub fn from_code(code: &str) -> Option<Lang> {
        let code = code.to_ascii_lowercase();
        let base = code.split(['-', '_', '.']).next().unwrap_or("");
        Lang::ALL.into_iter().find(|l| l.code() == base)
    }

    /// The system language, or English when it is not one of ours.
    pub fn detect() -> Lang {
        sys_locale::get_locale()
            .and_then(|l| Lang::from_code(&l))
            .unwrap_or(Lang::En)
    }

    fn index(self) -> usize {
        Lang::ALL.iter().position(|l| *l == self).unwrap()
    }
}

/// Text in `lang`.
pub fn t(lang: Lang, key: K) -> &'static str {
    table(key)[lang.index()]
}

/// Text in `lang` with `{placeholders}` filled in.
pub fn tf(lang: Lang, key: K, args: &[(&str, &str)]) -> String {
    let mut s = t(lang, key).to_string();
    for (k, v) in args {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

macro_rules! texts {
    ($($key:ident => [$it:expr, $en:expr, $de:expr, $es:expr, $fr:expr, $ru:expr, $zh:expr, $ja:expr $(,)?]),* $(,)?) => {
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        pub enum K { $($key),* }

        impl K {
            #[cfg(test)]
            pub const ALL: &'static [K] = &[$(K::$key),*];
        }

        fn table(key: K) -> [&'static str; 8] {
            match key {
                $(K::$key => [$it, $en, $de, $es, $fr, $ru, $zh, $ja]),*
            }
        }
    };
}

texts! {
    Language => ["Lingua", "Language", "Sprache", "Idioma", "Langue", "Язык", "语言", "言語"],
    TabMyPcs => ["I miei PC", "My PCs", "Meine PCs", "Mis PC", "Mes PC", "Мои ПК", "我的电脑", "マイPC"],
    TabShare => ["Condividi questo PC", "Share this PC", "Diesen PC freigeben", "Compartir este PC", "Partager ce PC", "Открыть доступ к этому ПК", "共享此电脑", "このPCを共有"],
    TabWake => ["Risveglio", "Wake-up", "Aufwecken", "Encendido remoto", "Réveil", "Пробуждение", "远程唤醒", "リモート起動"],
    AddPc => ["+ Aggiungi PC", "+ Add PC", "+ PC hinzufügen", "+ Añadir PC", "+ Ajouter un PC", "+ Добавить ПК", "+ 添加电脑", "+ PCを追加"],
    NoPcsYet => ["Nessun PC ancora.", "No PCs yet.", "Noch keine PCs.", "Todavía no hay ningún PC.", "Aucun PC pour l'instant.", "Пока нет ни одного ПК.", "还没有电脑。", "まだPCがありません。"],
    NoPcsHelp => [
        "Sul PC da controllare apri Magic Remote, scheda \"Condividi questo PC\", e premi Avvia. Poi qui premi \"Aggiungi PC\" e inserisci indirizzo, impronta e password che vedi là.",
        "On the PC you want to control, open Magic Remote, go to \"Share this PC\" and press Start. Then press \"Add PC\" here and enter the address, fingerprint and password shown there.",
        "Öffne auf dem PC, den du steuern willst, Magic Remote, gehe zu \"Diesen PC freigeben\" und klicke auf Starten. Klicke dann hier auf \"PC hinzufügen\" und gib Adresse, Fingerabdruck und Passwort von dort ein.",
        "En el PC que quieres controlar, abre Magic Remote, ve a \"Compartir este PC\" y pulsa Iniciar. Después pulsa aquí \"Añadir PC\" e introduce la dirección, la huella y la contraseña que aparecen allí.",
        "Sur le PC à contrôler, ouvrez Magic Remote, onglet « Partager ce PC », et cliquez sur Démarrer. Puis cliquez ici sur « Ajouter un PC » et saisissez l'adresse, l'empreinte et le mot de passe affichés là-bas.",
        "На ПК, которым нужно управлять, откройте Magic Remote, вкладку «Открыть доступ к этому ПК», и нажмите «Запустить». Затем нажмите здесь «Добавить ПК» и введите адрес, отпечаток и пароль, показанные там.",
        "在要控制的电脑上打开 Magic Remote，进入“共享此电脑”并点击“开始”。然后在这里点击“添加电脑”，输入那里显示的地址、指纹和密码。",
        "操作したいPCで Magic Remote を開き、「このPCを共有」で「開始」を押します。次にここで「PCを追加」を押し、そこに表示されたアドレス、フィンガープリント、パスワードを入力します。",
    ],
    StateOn => ["acceso", "on", "eingeschaltet", "encendido", "allumé", "включён", "已开机", "オン"],
    StateOff => ["spento o non raggiungibile", "off or unreachable", "aus oder nicht erreichbar", "apagado o inaccesible", "éteint ou injoignable", "выключен или недоступен", "已关机或无法访问", "オフまたは接続不可"],
    StateChecking => ["controllo…", "checking…", "wird geprüft…", "comprobando…", "vérification…", "проверка…", "检查中…", "確認中…"],
    WakeInternet => ["risvegliabile anche da Internet", "can be woken from the Internet", "auch über das Internet aufweckbar", "se puede encender desde Internet", "réveillable depuis Internet", "можно разбудить через интернет", "可通过互联网唤醒", "インターネットから起動可能"],
    WakeLan => ["risvegliabile dalla rete locale", "can be woken from the local network", "im lokalen Netzwerk aufweckbar", "se puede encender desde la red local", "réveillable depuis le réseau local", "можно разбудить из локальной сети", "可在局域网内唤醒", "ローカルネットワークから起動可能"],
    WakeNone => ["risveglio non disponibile", "wake-up not available", "Aufwecken nicht verfügbar", "encendido remoto no disponible", "réveil indisponible", "пробуждение недоступно", "不支持远程唤醒", "リモート起動は使用不可"],
    NetMapped => ["{addr} (porta aperta sul router con UPnP)", "{addr} (port opened on the router with UPnP)", "{addr} (Port per UPnP am Router geöffnet)", "{addr} (puerto abierto en el router con UPnP)", "{addr} (port ouvert sur le routeur via UPnP)", "{addr} (порт открыт на роутере через UPnP)", "{addr}（已通过 UPnP 在路由器上开放端口）", "{addr}（UPnPでルーターのポートを開放済み）"],
    NetCgnat => [
        "non raggiungibile: l'indirizzo Internet del router {ip} è privato (CGNAT o doppio router)",
        "not reachable: the router's Internet address {ip} is private (carrier-grade NAT or double router)",
        "nicht erreichbar: die Internetadresse {ip} des Routers ist privat (CGNAT oder doppelter Router)",
        "no accesible: la dirección de Internet del router {ip} es privada (CGNAT o doble router)",
        "injoignable : l'adresse Internet du routeur {ip} est privée (CGNAT ou double routeur)",
        "недоступен: интернет-адрес роутера {ip} частный (CGNAT или двойной роутер)",
        "无法访问：路由器的互联网地址 {ip} 是私有地址（运营商级 NAT 或双重路由器）",
        "接続不可：ルーターのインターネットアドレス {ip} がプライベートです（CGNATまたは二重ルーター）",
    ],
    NetNoUpnp => [
        "non raggiungibile: nessun router ha risposto via UPnP (attiva UPnP o apri la porta a mano)",
        "not reachable: no router answered UPnP (enable UPnP or open the port by hand)",
        "nicht erreichbar: kein Router hat auf UPnP geantwortet (UPnP aktivieren oder Port manuell öffnen)",
        "no accesible: ningún router respondió a UPnP (activa UPnP o abre el puerto a mano)",
        "injoignable : aucun routeur n'a répondu en UPnP (activez UPnP ou ouvrez le port à la main)",
        "недоступен: роутер не ответил по UPnP (включите UPnP или откройте порт вручную)",
        "无法访问：没有路由器响应 UPnP（请启用 UPnP 或手动开放端口）",
        "接続不可：UPnPに応答するルーターがありません（UPnPを有効にするか手動でポートを開放）",
    ],
    NetFailed => ["non raggiungibile: il router ha rifiutato di aprire la porta ({err})", "not reachable: the router refused to open the port ({err})", "nicht erreichbar: der Router hat das Öffnen des Ports abgelehnt ({err})", "no accesible: el router se negó a abrir el puerto ({err})", "injoignable : le routeur a refusé d'ouvrir le port ({err})", "недоступен: роутер отказался открыть порт ({err})", "无法访问：路由器拒绝开放端口（{err}）", "接続不可：ルーターがポート開放を拒否しました（{err}）"],
    NetDisabled => ["UPnP disattivato", "UPnP disabled", "UPnP deaktiviert", "UPnP desactivado", "UPnP désactivé", "UPnP отключён", "UPnP 已禁用", "UPnP無効"],
    Waking => ["Risveglio in corso…", "Waking up…", "Wird aufgeweckt…", "Encendiendo…", "Réveil en cours…", "Пробуждение…", "正在唤醒…", "起動中…"],
    Connecting => ["Connessione…", "Connecting…", "Verbinde…", "Conectando…", "Connexion…", "Подключение…", "正在连接…", "接続中…"],
    Connect => ["Connetti", "Connect", "Verbinden", "Conectar", "Se connecter", "Подключиться", "连接", "接続"],
    WakeAndConnect => ["Sveglia e connetti", "Wake and connect", "Aufwecken und verbinden", "Encender y conectar", "Réveiller et se connecter", "Разбудить и подключиться", "唤醒并连接", "起動して接続"],
    AddTitle => ["Aggiungi un PC", "Add a PC", "PC hinzufügen", "Añadir un PC", "Ajouter un PC", "Добавить ПК", "添加电脑", "PCを追加"],
    AddHelp => [
        "Copia questi dati dalla scheda \"Condividi questo PC\" dell'altro PC.",
        "Copy these details from the \"Share this PC\" tab on the other PC.",
        "Übernimm diese Angaben aus dem Reiter \"Diesen PC freigeben\" auf dem anderen PC.",
        "Copia estos datos de la pestaña \"Compartir este PC\" del otro PC.",
        "Recopiez ces informations depuis l'onglet « Partager ce PC » de l'autre PC.",
        "Скопируйте эти данные со вкладки «Открыть доступ к этому ПК» на другом ПК.",
        "从另一台电脑的“共享此电脑”页面复制这些信息。",
        "もう一方のPCの「このPCを共有」タブからこれらの情報をコピーしてください。",
    ],
    Address => ["Indirizzo", "Address", "Adresse", "Dirección", "Adresse", "Адрес", "地址", "アドレス"],
    Fingerprint => ["Impronta", "Fingerprint", "Fingerabdruck", "Huella", "Empreinte", "Отпечаток", "指纹", "フィンガープリント"],
    Password => ["Password", "Password", "Passwort", "Contraseña", "Mot de passe", "Пароль", "密码", "パスワード"],
    PairCheckbox => [
        "Abbina questo PC (le prossime volte niente password)",
        "Pair this PC (no password next time)",
        "Diesen PC koppeln (nächstes Mal ohne Passwort)",
        "Vincular este PC (la próxima vez sin contraseña)",
        "Associer ce PC (plus de mot de passe la prochaine fois)",
        "Связать этот ПК (в следующий раз без пароля)",
        "配对此电脑（下次无需密码）",
        "このPCをペアリング（次回からパスワード不要）",
    ],
    FpTooShort => [
        "L'impronta deve avere almeno 32 caratteri (8 gruppi da 4).",
        "The fingerprint needs at least 32 characters (8 groups of 4).",
        "Der Fingerabdruck braucht mindestens 32 Zeichen (8 Vierergruppen).",
        "La huella necesita al menos 32 caracteres (8 grupos de 4).",
        "L'empreinte doit contenir au moins 32 caractères (8 groupes de 4).",
        "Отпечаток должен содержать не менее 32 символов (8 групп по 4).",
        "指纹至少需要 32 个字符（8 组，每组 4 个）。",
        "フィンガープリントは32文字以上必要です（4文字×8グループ）。",
    ],
    PwTitle => ["Password per {name}", "Password for {name}", "Passwort für {name}", "Contraseña de {name}", "Mot de passe pour {name}", "Пароль для {name}", "{name} 的密码", "{name} のパスワード"],
    PwHelp => [
        "Questo PC non è ancora abbinato: inserisci la password che mostra.",
        "This PC is not paired yet: enter the password it shows.",
        "Dieser PC ist noch nicht gekoppelt: Gib das dort angezeigte Passwort ein.",
        "Este PC aún no está vinculado: introduce la contraseña que muestra.",
        "Ce PC n'est pas encore associé : saisissez le mot de passe qu'il affiche.",
        "Этот ПК ещё не связан: введите пароль, который он показывает.",
        "此电脑尚未配对：请输入它显示的密码。",
        "このPCはまだペアリングされていません。表示されているパスワードを入力してください。",
    ],
    PairShort => ["Abbina, così la prossima volta non serve", "Pair, so it is not needed next time", "Koppeln, damit es nächstes Mal nicht nötig ist", "Vincular, para no necesitarla la próxima vez", "Associer, pour ne plus en avoir besoin", "Связать, чтобы в следующий раз не вводить", "配对，下次就不需要了", "ペアリングして次回から不要にする"],
    ThisFp => ["Impronta di questo PC: {fp}", "This PC's fingerprint: {fp}", "Fingerabdruck dieses PCs: {fp}", "Huella de este PC: {fp}", "Empreinte de ce PC : {fp}", "Отпечаток этого ПК: {fp}", "此电脑的指纹：{fp}", "このPCのフィンガープリント：{fp}"],
    ErrWrongPw => ["password sbagliata", "wrong password", "falsches Passwort", "contraseña incorrecta", "mot de passe incorrect", "неверный пароль", "密码错误", "パスワードが違います"],
    ErrBusy => ["è già collegato un altro PC", "another PC is already connected", "ein anderer PC ist bereits verbunden", "ya hay otro PC conectado", "un autre PC est déjà connecté", "уже подключён другой ПК", "已有另一台电脑连接", "別のPCが既に接続しています"],
    ErrFp => [
        "l'impronta non corrisponde: controlla di aver copiato quella giusta",
        "the fingerprint does not match: check that you copied the right one",
        "der Fingerabdruck stimmt nicht: Prüfe, ob du den richtigen kopiert hast",
        "la huella no coincide: comprueba que copiaste la correcta",
        "l'empreinte ne correspond pas : vérifiez que vous avez copié la bonne",
        "отпечаток не совпадает: проверьте, что скопировали правильный",
        "指纹不匹配：请确认复制的是正确的指纹",
        "フィンガープリントが一致しません。正しいものをコピーしたか確認してください",
    ],
    ErrNoAnswer => [
        "non risponde: è acceso, con la condivisione avviata e il firewall che consente la porta UDP 47800?",
        "no answer: is it on, with sharing started and UDP port 47800 allowed by the firewall?",
        "keine Antwort: Ist er eingeschaltet, die Freigabe gestartet und UDP-Port 47800 in der Firewall erlaubt?",
        "no responde: ¿está encendido, con el uso compartido iniciado y el puerto UDP 47800 permitido en el firewall?",
        "pas de réponse : est-il allumé, avec le partage démarré et le port UDP 47800 autorisé par le pare-feu ?",
        "нет ответа: он включён, доступ запущен и UDP-порт 47800 разрешён в брандмауэре?",
        "没有响应：它是否已开机、已开始共享，并且防火墙允许 UDP 端口 47800？",
        "応答がありません。電源が入っていて共有が開始され、ファイアウォールでUDPポート47800が許可されていますか？",
    ],
    ErrPairedOnly => ["accetta solo PC abbinati", "it only accepts paired PCs", "er akzeptiert nur gekoppelte PCs", "solo acepta PC vinculados", "il n'accepte que les PC associés", "он принимает только связанные ПК", "它只接受已配对的电脑", "ペアリング済みのPCのみ受け付けます"],
    ErrSpawn => ["Impossibile avviare la connessione: {e}", "Cannot start the connection: {e}", "Verbindung kann nicht gestartet werden: {e}", "No se puede iniciar la conexión: {e}", "Impossible de lancer la connexion : {e}", "Не удалось начать подключение: {e}", "无法开始连接：{e}", "接続を開始できません：{e}"],
    IsOn => ["{name} è acceso", "{name} is on", "{name} ist eingeschaltet", "{name} está encendido", "{name} est allumé", "{name} включён", "{name} 已开机", "{name} が起動しました"],
    WakeFailed => [
        "{name} non si è svegliato entro 2 minuti. Controlla la scheda Risveglio sul PC di casa.",
        "{name} did not wake up within 2 minutes. Check the Wake-up tab on that PC.",
        "{name} ist nicht innerhalb von 2 Minuten aufgewacht. Prüfe den Reiter Aufwecken auf diesem PC.",
        "{name} no se encendió en 2 minutos. Revisa la pestaña Encendido remoto en ese PC.",
        "{name} ne s'est pas réveillé en 2 minutes. Vérifiez l'onglet Réveil sur ce PC.",
        "{name} не проснулся за 2 минуты. Проверьте вкладку «Пробуждение» на том ПК.",
        "{name} 在 2 分钟内没有唤醒。请检查那台电脑上的“远程唤醒”页面。",
        "{name} は2分以内に起動しませんでした。そのPCの「リモート起動」タブを確認してください。",
    ],
    ShareHelp => [
        "Permette a un altro PC di vedere questo schermo e usare mouse e tastiera.",
        "Lets another PC see this screen and use the mouse and keyboard.",
        "Erlaubt einem anderen PC, diesen Bildschirm zu sehen und Maus und Tastatur zu benutzen.",
        "Permite que otro PC vea esta pantalla y use el ratón y el teclado.",
        "Permet à un autre PC de voir cet écran et d'utiliser la souris et le clavier.",
        "Позволяет другому ПК видеть этот экран и управлять мышью и клавиатурой.",
        "允许另一台电脑查看此屏幕并使用鼠标和键盘。",
        "別のPCがこの画面を表示し、マウスとキーボードを操作できるようにします。",
    ],
    PairedOnlyCheckbox => [
        "Solo PC abbinati (nessuna password, consigliato se lo lasci acceso)",
        "Paired PCs only (no password, recommended if you leave it on)",
        "Nur gekoppelte PCs (kein Passwort, empfohlen, wenn er eingeschaltet bleibt)",
        "Solo PC vinculados (sin contraseña, recomendado si lo dejas encendido)",
        "PC associés uniquement (sans mot de passe, conseillé s'il reste allumé)",
        "Только связанные ПК (без пароля, рекомендуется, если ПК остаётся включённым)",
        "仅限已配对的电脑（无需密码，长时间开机时推荐）",
        "ペアリング済みPCのみ（パスワードなし、電源を入れたままにする場合に推奨）",
    ],
    StartSharing => ["Avvia condivisione", "Start sharing", "Freigabe starten", "Iniciar uso compartido", "Démarrer le partage", "Запустить доступ", "开始共享", "共有を開始"],
    StopSharing => ["Ferma condivisione", "Stop sharing", "Freigabe beenden", "Detener uso compartido", "Arrêter le partage", "Остановить доступ", "停止共享", "共有を停止"],
    Error => ["Errore: {e}", "Error: {e}", "Fehler: {e}", "Error: {e}", "Erreur : {e}", "Ошибка: {e}", "错误：{e}", "エラー：{e}"],
    Starting => ["Avvio in corso (apertura della porta sul router)…", "Starting (opening the port on the router)…", "Wird gestartet (Port am Router wird geöffnet)…", "Iniciando (abriendo el puerto en el router)…", "Démarrage (ouverture du port sur le routeur)…", "Запуск (открытие порта на роутере)…", "正在启动（在路由器上打开端口）…", "起動中（ルーターのポートを開いています）…"],
    FromInternet => ["Da Internet", "From the Internet", "Aus dem Internet", "Desde Internet", "Depuis Internet", "Из интернета", "来自互联网", "インターネットから"],
    Now => ["Ora", "Now", "Jetzt", "Ahora", "Maintenant", "Сейчас", "当前", "現在"],
    ViewerConnected => ["collegato: {v}", "connected: {v}", "verbunden: {v}", "conectado: {v}", "connecté : {v}", "подключён: {v}", "已连接：{v}", "接続中：{v}"],
    WaitingConnections => ["in attesa di connessioni", "waiting for connections", "wartet auf Verbindungen", "esperando conexiones", "en attente de connexions", "ожидание подключений", "等待连接", "接続を待っています"],
    PasswordDisabled => ["disattivata: solo PC abbinati", "off: paired PCs only", "aus: nur gekoppelte PCs", "desactivada: solo PC vinculados", "désactivé : PC associés uniquement", "отключён: только связанные ПК", "已关闭：仅限已配对的电脑", "オフ：ペアリング済みPCのみ"],
    Activity => ["Attività", "Activity", "Aktivität", "Actividad", "Activité", "События", "活动", "アクティビティ"],
    PairedPcs => ["PC abbinati", "Paired PCs", "Gekoppelte PCs", "PC vinculados", "PC associés", "Связанные ПК", "已配对的电脑", "ペアリング済みPC"],
    NoPaired => [
        "Nessuno. Un PC si abbina la prima volta che si collega con la password.",
        "None. A PC gets paired the first time it connects with the password.",
        "Keine. Ein PC wird gekoppelt, wenn er sich das erste Mal mit Passwort verbindet.",
        "Ninguno. Un PC se vincula la primera vez que se conecta con la contraseña.",
        "Aucun. Un PC est associé la première fois qu'il se connecte avec le mot de passe.",
        "Нет. ПК связывается при первом подключении с паролем.",
        "暂无。电脑第一次使用密码连接时会完成配对。",
        "なし。PCは初めてパスワードで接続したときにペアリングされます。",
    ],
    Revoke => ["Revoca", "Revoke", "Entfernen", "Revocar", "Révoquer", "Отозвать", "撤销", "解除"],
    Revoked => ["Revocato: {name}", "Revoked: {name}", "Entfernt: {name}", "Revocado: {name}", "Révoqué : {name}", "Отозван: {name}", "已撤销：{name}", "解除しました：{name}"],
    Copy => ["Copia", "Copy", "Kopieren", "Copiar", "Copier", "Копировать", "复制", "コピー"],
    WakeTitle => ["Risveglio da remoto", "Remote wake-up", "Aufwecken aus der Ferne", "Encendido remoto", "Réveil à distance", "Удалённое пробуждение", "远程唤醒", "リモート起動"],
    WakeHelp => [
        "Per raggiungere questo PC da fuori casa senza lasciarlo acceso: mettilo in sospensione (non spegnerlo), con la condivisione avviata. Dagli altri PC il pulsante \"Sveglia e connetti\" lo riaccende.",
        "To reach this PC from away without leaving it on: put it to sleep (do not shut it down) with sharing started. On your other PCs, \"Wake and connect\" turns it back on.",
        "Um diesen PC unterwegs zu erreichen, ohne ihn eingeschaltet zu lassen: Versetze ihn in den Energiesparmodus (nicht herunterfahren), während die Freigabe läuft. Auf den anderen PCs weckt ihn \"Aufwecken und verbinden\" wieder auf.",
        "Para acceder a este PC desde fuera sin dejarlo encendido: ponlo en suspensión (no lo apagues) con el uso compartido iniciado. Desde tus otros PC, \"Encender y conectar\" lo vuelve a encender.",
        "Pour joindre ce PC à distance sans le laisser allumé : mettez-le en veille (ne l'éteignez pas) avec le partage démarré. Depuis vos autres PC, « Réveiller et se connecter » le rallume.",
        "Чтобы подключаться к этому ПК издалека, не оставляя его включённым: переведите его в спящий режим (не выключайте) с запущенным доступом. На других ПК кнопка «Разбудить и подключиться» включит его.",
        "要在外面访问这台电脑又不让它一直开着：在共享已开始的情况下让它进入睡眠（不要关机）。在你的其他电脑上点击“唤醒并连接”即可将它唤醒。",
        "電源を入れたままにせずに外出先からこのPCに接続するには、共有を開始した状態でスリープにします（シャットダウンしないでください）。ほかのPCで「起動して接続」を押すと起動します。",
    ],
    CheckThisPc => ["Controlla questo PC", "Check this PC", "Diesen PC prüfen", "Comprobar este PC", "Vérifier ce PC", "Проверить этот ПК", "检查此电脑", "このPCを確認"],
    FixSettings => ["Correggi le impostazioni (amministratore)", "Fix the settings (administrator)", "Einstellungen korrigieren (Administrator)", "Corregir la configuración (administrador)", "Corriger les réglages (administrateur)", "Исправить настройки (администратор)", "修复设置（管理员）", "設定を修正（管理者）"],
    FixHover => [
        "Attiva il risveglio sulla scheda di rete. Il sistema chiederà il permesso.",
        "Turns on wake-up on the network card. The system will ask for permission.",
        "Aktiviert das Aufwecken an der Netzwerkkarte. Das System fragt nach der Berechtigung.",
        "Activa el encendido en la tarjeta de red. El sistema pedirá permiso.",
        "Active le réveil sur la carte réseau. Le système demandera l'autorisation.",
        "Включает пробуждение на сетевой карте. Система запросит разрешение.",
        "在网卡上启用唤醒功能。系统会请求权限。",
        "ネットワークカードの起動機能を有効にします。システムが許可を求めます。",
    ],
    VerdictInternet => ["Questo PC si può svegliare anche da Internet.", "This PC can be woken from the Internet.", "Dieser PC kann auch über das Internet aufgeweckt werden.", "Este PC se puede encender también desde Internet.", "Ce PC peut être réveillé depuis Internet.", "Этот ПК можно разбудить через интернет.", "这台电脑可以通过互联网唤醒。", "このPCはインターネットから起動できます。"],
    VerdictLan => ["Questo PC si può svegliare solo dalla rete di casa.", "This PC can be woken only from the home network.", "Dieser PC kann nur im Heimnetz aufgeweckt werden.", "Este PC solo se puede encender desde la red de casa.", "Ce PC ne peut être réveillé que depuis le réseau local.", "Этот ПК можно разбудить только из домашней сети.", "这台电脑只能在家庭网络内唤醒。", "このPCはホームネットワークからのみ起動できます。"],
    VerdictNotReady => ["Non ancora pronto: vedi i punti in rosso.", "Not ready yet: see the items in red.", "Noch nicht bereit: siehe die roten Punkte.", "Todavía no está listo: revisa los puntos en rojo.", "Pas encore prêt : voir les points en rouge.", "Ещё не готово: см. пункты, отмеченные красным.", "尚未就绪：请查看红色项目。", "まだ準備ができていません。赤い項目を確認してください。"],
    FixFailed => ["Correzione non eseguita (permesso negato o non disponibile).", "Fix not applied (permission denied or not available).", "Korrektur nicht ausgeführt (Berechtigung verweigert oder nicht verfügbar).", "Corrección no aplicada (permiso denegado o no disponible).", "Correction non appliquée (autorisation refusée ou indisponible).", "Исправление не выполнено (нет разрешения или недоступно).", "未执行修复（权限被拒绝或不可用）。", "修正されませんでした（権限が拒否されたか、利用できません）。"],
    // ─── Sicurezza ───
    TabSecurity => ["Sicurezza", "Security", "Sicherheit", "Seguridad", "Sécurité", "Безопасность", "安全", "セキュリティ"],
    SecAccessTitle => ["Password di accesso", "Access password", "Zugangspasswort", "Contraseña de acceso", "Mot de passe d'accès", "Пароль доступа", "访问密码", "アクセスパスワード"],
    SecAccessHelp => [
        "La chiedono i PC non ancora abbinati quando si collegano a questo.",
        "PCs that are not paired yet must type it to connect to this one.",
        "Noch nicht gekoppelte PCs müssen es eingeben, um sich mit diesem zu verbinden.",
        "Los PC aún no vinculados deben escribirla para conectarse a este.",
        "Les PC pas encore associés doivent le saisir pour se connecter à celui-ci.",
        "Его вводят ещё не связанные ПК, чтобы подключиться к этому.",
        "尚未配对的电脑连接到本机时需要输入它。",
        "まだペアリングしていないPCがこのPCに接続するときに入力します。",
    ],
    SecRandomEach => ["Nuova password casuale a ogni avvio", "New random password at each start", "Bei jedem Start ein neues Zufallspasswort", "Nueva contraseña aleatoria en cada inicio", "Nouveau mot de passe aléatoire à chaque démarrage", "Новый случайный пароль при каждом запуске", "每次启动生成新的随机密码", "起動するたびに新しいランダムパスワード"],
    SecFixed => ["Password fissa scelta da me", "A fixed password I choose", "Ein festes Passwort, das ich wähle", "Una contraseña fija elegida por mí", "Un mot de passe fixe choisi par moi", "Постоянный пароль, который я выберу", "我自己设置的固定密码", "自分で決めた固定パスワード"],
    SecFixedSet => [
        "✓ Password fissa impostata. Viene salvata solo la sua impronta Argon2id, mai la password.",
        "✓ Fixed password set. Only its Argon2id hash is stored, never the password.",
        "✓ Festes Passwort gesetzt. Gespeichert wird nur sein Argon2id-Hash, nie das Passwort.",
        "✓ Contraseña fija establecida. Solo se guarda su hash Argon2id, nunca la contraseña.",
        "✓ Mot de passe fixe défini. Seule son empreinte Argon2id est enregistrée, jamais le mot de passe.",
        "✓ Постоянный пароль задан. Хранится только его хеш Argon2id, а не сам пароль.",
        "✓ 已设置固定密码。仅保存其 Argon2id 哈希，从不保存密码本身。",
        "✓ 固定パスワードを設定済み。保存されるのは Argon2id ハッシュだけで、パスワード自体は保存されません。",
    ],
    ChangePassword => ["Cambia password", "Change password", "Passwort ändern", "Cambiar contraseña", "Changer le mot de passe", "Сменить пароль", "更改密码", "パスワードを変更"],
    NewPassword => ["Nuova password", "New password", "Neues Passwort", "Nueva contraseña", "Nouveau mot de passe", "Новый пароль", "新密码", "新しいパスワード"],
    RepeatPassword => ["Ripeti", "Repeat", "Wiederholen", "Repetir", "Répéter", "Повторите", "再次输入", "もう一度"],
    Save => ["Salva", "Save", "Speichern", "Guardar", "Enregistrer", "Сохранить", "保存", "保存"],
    Cancel => ["Annulla", "Cancel", "Abbrechen", "Cancelar", "Annuler", "Отмена", "取消", "キャンセル"],
    Remove => ["Rimuovi", "Remove", "Entfernen", "Quitar", "Supprimer", "Удалить", "移除", "削除"],
    PwMismatch => ["Le due password non coincidono.", "The two passwords differ.", "Die beiden Passwörter stimmen nicht überein.", "Las dos contraseñas no coinciden.", "Les deux mots de passe ne correspondent pas.", "Пароли не совпадают.", "两次输入的密码不一致。", "2つのパスワードが一致しません。"],
    PwMinLen => ["Almeno {n} caratteri.", "At least {n} characters.", "Mindestens {n} Zeichen.", "Al menos {n} caracteres.", "Au moins {n} caractères.", "Не менее {n} символов.", "至少 {n} 个字符。", "{n} 文字以上。"],
    PwTooWeak => [
        "Troppo facile da indovinare: allungala o mescola lettere, numeri e simboli.",
        "Too easy to guess: make it longer, or mix letters, digits and symbols.",
        "Zu leicht zu erraten: mach es länger oder mische Buchstaben, Ziffern und Zeichen.",
        "Demasiado fácil de adivinar: hazla más larga o mezcla letras, números y símbolos.",
        "Trop facile à deviner : allongez-le ou mélangez lettres, chiffres et symboles.",
        "Слишком легко угадать: сделайте длиннее или смешайте буквы, цифры и символы.",
        "太容易被猜到：请加长，或混合使用字母、数字和符号。",
        "推測されやすすぎます。長くするか、文字・数字・記号を組み合わせてください。",
    ],
    StrTooShort => ["troppo corta", "too short", "zu kurz", "demasiado corta", "trop court", "слишком короткий", "太短", "短すぎます"],
    StrWeak => ["debole", "weak", "schwach", "débil", "faible", "слабый", "弱", "弱い"],
    StrMedium => ["discreta", "fair", "mittel", "aceptable", "moyen", "средний", "中等", "普通"],
    StrStrong => ["forte", "strong", "stark", "fuerte", "fort", "надёжный", "强", "強い"],
    PwSaved => ["Password fissa salvata.", "Fixed password saved.", "Festes Passwort gespeichert.", "Contraseña fija guardada.", "Mot de passe fixe enregistré.", "Постоянный пароль сохранён.", "固定密码已保存。", "固定パスワードを保存しました。"],
    PwRemoved => ["Password fissa rimossa: ora ne viene creata una casuale a ogni avvio.", "Fixed password removed: a random one is now made at each start.", "Festes Passwort entfernt: jetzt wird bei jedem Start ein zufälliges erzeugt.", "Contraseña fija eliminada: ahora se crea una aleatoria en cada inicio.", "Mot de passe fixe supprimé : un mot de passe aléatoire est désormais créé à chaque démarrage.", "Постоянный пароль удалён: теперь при каждом запуске создаётся случайный.", "已移除固定密码：现在每次启动都会生成随机密码。", "固定パスワードを削除しました。今後は起動ごとにランダムなものが作られます。"],
    PasswordFixed => ["password fissa (vedi Sicurezza)", "fixed password (see Security)", "festes Passwort (siehe Sicherheit)", "contraseña fija (ver Seguridad)", "mot de passe fixe (voir Sécurité)", "постоянный пароль (см. «Безопасность»)", "固定密码（见“安全”）", "固定パスワード（「セキュリティ」を参照）"],
    SecApproveTitle => ["Conferma su questo PC", "Confirm on this PC", "Bestätigung an diesem PC", "Confirmación en este PC", "Confirmation sur ce PC", "Подтверждение на этом ПК", "在本机确认", "このPCで確認"],
    SecApproveCheckbox => [
        "Chiedimi il permesso prima di far entrare un PC non abbinato",
        "Ask me before letting in a PC that is not paired",
        "Mich fragen, bevor ein nicht gekoppelter PC hereingelassen wird",
        "Preguntarme antes de dejar entrar a un PC no vinculado",
        "Me demander avant de laisser entrer un PC non associé",
        "Спрашивать меня, прежде чем впустить не связанный ПК",
        "允许未配对的电脑进入前先询问我",
        "ペアリングしていないPCを入れる前に確認する",
    ],
    SecApproveHelp => [
        "Anche con la password giusta, qui comparirà una richiesta da accettare entro 60 secondi. Utile se la password potrebbe essere stata vista da altri; lascialo spento se devi collegarti quando qui non c'è nessuno.",
        "Even with the right password, a request appears here that must be accepted within 60 seconds. Useful if others may have seen the password; leave it off if you need to connect when nobody is here.",
        "Auch mit dem richtigen Passwort erscheint hier eine Anfrage, die innerhalb von 60 Sekunden bestätigt werden muss. Nützlich, wenn andere das Passwort gesehen haben könnten; lass es aus, wenn du dich verbinden musst, während niemand hier ist.",
        "Incluso con la contraseña correcta, aquí aparecerá una solicitud que hay que aceptar en 60 segundos. Útil si otros pueden haber visto la contraseña; déjalo desactivado si necesitas conectarte cuando aquí no hay nadie.",
        "Même avec le bon mot de passe, une demande apparaîtra ici, à accepter sous 60 secondes. Utile si d'autres ont pu voir le mot de passe ; laissez désactivé si vous devez vous connecter quand personne n'est là.",
        "Даже при верном пароле здесь появится запрос, который нужно принять за 60 секунд. Полезно, если пароль могли подсмотреть; оставьте выключенным, если нужно подключаться, когда здесь никого нет.",
        "即使密码正确，这里也会弹出请求，需要在 60 秒内接受。适合密码可能被他人看到的情况；如果需要在无人值守时连接，请保持关闭。",
        "パスワードが正しくても、ここに要求が表示され、60秒以内に許可する必要があります。パスワードを他人に見られた可能性があるときに便利です。誰もいないときに接続する必要があるならオフのままにしてください。",
    ],
    SecAttemptsTitle => ["Protezione dai tentativi", "Protection against guessing", "Schutz vor Rateversuchen", "Protección contra intentos", "Protection contre les tentatives", "Защита от подбора", "防暴力破解", "総当たり対策"],
    SecAttemptsHelp => [
        "Dopo {n} password sbagliate dallo stesso indirizzo, quell'indirizzo resta bloccato per {min} minuti, e il blocco raddoppia se continua. I PC abbinati non vengono mai bloccati.",
        "After {n} wrong passwords from the same address, that address is locked out for {min} minutes, and the lockout doubles if it goes on. Paired PCs are never locked out.",
        "Nach {n} falschen Passwörtern von derselben Adresse wird diese Adresse für {min} Minuten gesperrt, und die Sperre verdoppelt sich, wenn es weitergeht. Gekoppelte PCs werden nie gesperrt.",
        "Tras {n} contraseñas incorrectas desde la misma dirección, esa dirección queda bloqueada {min} minutos, y el bloqueo se duplica si continúa. Los PC vinculados nunca se bloquean.",
        "Après {n} mots de passe erronés depuis la même adresse, celle-ci est bloquée {min} minutes, et le blocage double si cela continue. Les PC associés ne sont jamais bloqués.",
        "После {n} неверных паролей с одного адреса он блокируется на {min} минут, а при повторении срок удваивается. Связанные ПК никогда не блокируются.",
        "同一地址输错密码 {n} 次后，该地址将被锁定 {min} 分钟，继续尝试则锁定时间加倍。已配对的电脑永远不会被锁定。",
        "同じアドレスから {n} 回パスワードを間違えると、そのアドレスは {min} 分間ブロックされ、続けると時間が倍になります。ペアリング済みのPCはブロックされません。",
    ],
    SecFailedCount => ["Password sbagliate finora: {n}", "Wrong passwords so far: {n}", "Bisher falsche Passwörter: {n}", "Contraseñas incorrectas hasta ahora: {n}", "Mots de passe erronés jusqu'ici : {n}", "Неверных паролей пока: {n}", "目前输错密码次数：{n}", "これまでの誤ったパスワード：{n}"],
    SecNoBlocked => ["Nessun indirizzo bloccato.", "No address locked out.", "Keine Adresse gesperrt.", "Ninguna dirección bloqueada.", "Aucune adresse bloquée.", "Заблокированных адресов нет.", "没有被锁定的地址。", "ブロック中のアドレスはありません。"],
    SecBlockedFor => ["{ip}: bloccato ancora per {wait}", "{ip}: locked out for {wait} more", "{ip}: noch {wait} gesperrt", "{ip}: bloqueada {wait} más", "{ip} : bloquée encore {wait}", "{ip}: заблокирован ещё на {wait}", "{ip}：还将锁定 {wait}", "{ip}：あと {wait} ブロック"],
    Unblock => ["Sblocca", "Unblock", "Entsperren", "Desbloquear", "Débloquer", "Разблокировать", "解除锁定", "解除"],
    SecPaused => [
        "Troppi tentativi da indirizzi diversi: accesso con password sospeso per tutti ancora per {wait}.",
        "Too many attempts from different addresses: password logins paused for everyone for {wait} more.",
        "Zu viele Versuche von verschiedenen Adressen: Anmeldung per Passwort für alle noch {wait} pausiert.",
        "Demasiados intentos desde direcciones distintas: acceso con contraseña suspendido para todos {wait} más.",
        "Trop de tentatives depuis des adresses différentes : connexion par mot de passe suspendue pour tous encore {wait}.",
        "Слишком много попыток с разных адресов: вход по паролю приостановлен для всех ещё на {wait}.",
        "来自不同地址的尝试过多：所有人的密码登录暂停，还剩 {wait}。",
        "複数のアドレスから試行が多すぎます。全員のパスワードログインをあと {wait} 停止中です。",
    ],
    Resume => ["Riattiva", "Resume", "Fortsetzen", "Reanudar", "Réactiver", "Возобновить", "恢复", "再開"],
    SecLockTitle => ["Blocco dell'app", "App lock", "App-Sperre", "Bloqueo de la app", "Verrouillage de l'app", "Блокировка приложения", "应用锁", "アプリのロック"],
    SecLockHelp => [
        "Chiede un PIN o una password per aprire Magic Remote, così chi usa questo PC non può collegarsi ai tuoi PC abbinati.",
        "Asks for a PIN or password to open Magic Remote, so whoever uses this PC cannot connect to your paired PCs.",
        "Verlangt eine PIN oder ein Passwort zum Öffnen von Magic Remote, damit niemand an diesem PC sich mit deinen gekoppelten PCs verbinden kann.",
        "Pide un PIN o contraseña para abrir Magic Remote, así quien use este PC no puede conectarse a tus PC vinculados.",
        "Demande un PIN ou un mot de passe pour ouvrir Magic Remote, afin que quiconque utilise ce PC ne puisse pas se connecter à vos PC associés.",
        "Запрашивает PIN или пароль при открытии Magic Remote, чтобы тот, кто пользуется этим ПК, не мог подключиться к вашим связанным ПК.",
        "打开 Magic Remote 时需要输入 PIN 或密码，这样使用本机的其他人就无法连接到你已配对的电脑。",
        "Magic Remote を開くときに PIN またはパスワードを求め、このPCを使う他の人がペアリング済みのPCに接続できないようにします。",
    ],
    SecLockNote => [
        "Il blocco protegge l'app aperta; non cifra i file di Magic Remote sul disco.",
        "The lock protects the open app; it does not encrypt Magic Remote's files on disk.",
        "Die Sperre schützt die geöffnete App; sie verschlüsselt die Dateien von Magic Remote auf der Festplatte nicht.",
        "El bloqueo protege la app abierta; no cifra los archivos de Magic Remote en el disco.",
        "Le verrouillage protège l'app ouverte ; il ne chiffre pas les fichiers de Magic Remote sur le disque.",
        "Блокировка защищает открытое приложение, но не шифрует файлы Magic Remote на диске.",
        "应用锁保护打开的应用，但不会加密磁盘上的 Magic Remote 文件。",
        "ロックは開いているアプリを守るもので、ディスク上の Magic Remote のファイルは暗号化しません。",
    ],
    SetLockPin => ["Imposta PIN di blocco", "Set a lock PIN", "Sperr-PIN festlegen", "Establecer PIN de bloqueo", "Définir un PIN de verrouillage", "Задать PIN блокировки", "设置锁定 PIN", "ロック用 PIN を設定"],
    SecLockOn => ["✓ Blocco attivo.", "✓ Lock on.", "✓ Sperre aktiv.", "✓ Bloqueo activado.", "✓ Verrouillage actif.", "✓ Блокировка включена.", "✓ 应用锁已开启。", "✓ ロック有効。"],
    NewPin => ["Nuovo PIN o password", "New PIN or password", "Neue PIN oder neues Passwort", "Nuevo PIN o contraseña", "Nouveau PIN ou mot de passe", "Новый PIN или пароль", "新 PIN 或密码", "新しい PIN またはパスワード"],
    CurrentPin => ["PIN attuale", "Current PIN", "Aktuelle PIN", "PIN actual", "PIN actuel", "Текущий PIN", "当前 PIN", "現在の PIN"],
    AutoLock => ["Blocca da solo dopo", "Lock by itself after", "Automatisch sperren nach", "Bloquear solo tras", "Verrouiller seul après", "Блокировать само через", "空闲多久后自动锁定", "自動ロックまでの時間"],
    Never => ["mai", "never", "nie", "nunca", "jamais", "никогда", "从不", "しない"],
    Minutes => ["{n} min di inattività", "{n} min idle", "{n} Min. Inaktivität", "{n} min de inactividad", "{n} min d'inactivité", "{n} мин бездействия", "空闲 {n} 分钟", "{n} 分間操作なし"],
    LockNow => ["Blocca ora", "Lock now", "Jetzt sperren", "Bloquear ahora", "Verrouiller maintenant", "Заблокировать", "立即锁定", "今すぐロック"],
    ChangePin => ["Cambia PIN", "Change PIN", "PIN ändern", "Cambiar PIN", "Changer le PIN", "Сменить PIN", "更改 PIN", "PIN を変更"],
    PinSaved => ["PIN di blocco salvato.", "Lock PIN saved.", "Sperr-PIN gespeichert.", "PIN de bloqueo guardado.", "PIN de verrouillage enregistré.", "PIN блокировки сохранён.", "锁定 PIN 已保存。", "ロック用 PIN を保存しました。"],
    PinRemoved => ["Blocco dell'app rimosso.", "App lock removed.", "App-Sperre entfernt.", "Bloqueo de la app quitado.", "Verrouillage de l'app supprimé.", "Блокировка приложения снята.", "已移除应用锁。", "アプリのロックを解除しました。"],
    WrongPin => ["PIN sbagliato.", "Wrong PIN.", "Falsche PIN.", "PIN incorrecto.", "PIN incorrect.", "Неверный PIN.", "PIN 错误。", "PIN が違います。"],
    LockedTitle => ["Magic Remote è bloccato", "Magic Remote is locked", "Magic Remote ist gesperrt", "Magic Remote está bloqueado", "Magic Remote est verrouillé", "Magic Remote заблокирован", "Magic Remote 已锁定", "Magic Remote はロック中です"],
    LockedHelp => ["Inserisci il PIN o la password per sbloccarlo.", "Enter the PIN or password to unlock it.", "Gib die PIN oder das Passwort ein, um es zu entsperren.", "Introduce el PIN o la contraseña para desbloquearlo.", "Saisissez le PIN ou le mot de passe pour le déverrouiller.", "Введите PIN или пароль, чтобы разблокировать.", "输入 PIN 或密码以解锁。", "PIN またはパスワードを入力してロックを解除してください。"],
    Unlock => ["Sblocca", "Unlock", "Entsperren", "Desbloquear", "Déverrouiller", "Разблокировать", "解锁", "ロック解除"],
    LockedWait => ["Troppi tentativi. Riprova tra {wait}.", "Too many tries. Try again in {wait}.", "Zu viele Versuche. Erneut versuchen in {wait}.", "Demasiados intentos. Vuelve a intentarlo en {wait}.", "Trop d'essais. Réessayez dans {wait}.", "Слишком много попыток. Повторите через {wait}.", "尝试次数过多。请在 {wait} 后重试。", "試行回数が多すぎます。{wait} 後にもう一度お試しください。"],
    LockedSharing => ["La condivisione di questo PC resta attiva mentre l'app è bloccata.", "Sharing this PC stays on while the app is locked.", "Die Freigabe dieses PCs bleibt aktiv, während die App gesperrt ist.", "El uso compartido de este PC sigue activo mientras la app está bloqueada.", "Le partage de ce PC reste actif pendant que l'app est verrouillée.", "Доступ к этому ПК остаётся открытым, пока приложение заблокировано.", "应用锁定期间，本机共享保持开启。", "アプリがロック中でも、このPCの共有は続きます。"],
    LockedRequest => ["{who} chiede di collegarsi: sblocca per rispondere.", "{who} asks to connect: unlock to answer.", "{who} möchte sich verbinden: entsperren, um zu antworten.", "{who} pide conectarse: desbloquea para responder.", "{who} demande à se connecter : déverrouillez pour répondre.", "{who} просит подключиться: разблокируйте, чтобы ответить.", "{who} 请求连接：请解锁后回应。", "{who} が接続を求めています。応答するにはロックを解除してください。"],
    ApproveTitle => ["Richiesta di accesso", "Access request", "Zugriffsanfrage", "Solicitud de acceso", "Demande d'accès", "Запрос доступа", "访问请求", "アクセス要求"],
    ApproveText => [
        "{who} ha inserito la password giusta e chiede di controllare questo PC.",
        "{who} typed the right password and asks to control this PC.",
        "{who} hat das richtige Passwort eingegeben und möchte diesen PC steuern.",
        "{who} escribió la contraseña correcta y pide controlar este PC.",
        "{who} a saisi le bon mot de passe et demande à contrôler ce PC.",
        "{who} ввёл верный пароль и просит управлять этим ПК.",
        "{who} 输入了正确的密码，请求控制本机。",
        "{who} が正しいパスワードを入力し、このPCの操作を求めています。",
    ],
    ApproveCountdown => ["Se non rispondi entro {s} s viene rifiutato.", "Refused unless you answer within {s} s.", "Wird abgelehnt, wenn du nicht innerhalb von {s} s antwortest.", "Se rechaza si no respondes en {s} s.", "Refusé si vous ne répondez pas sous {s} s.", "Будет отклонено, если не ответить за {s} с.", "{s} 秒内未回应将自动拒绝。", "{s} 秒以内に応答しないと拒否されます。"],
    Allow => ["Consenti", "Allow", "Zulassen", "Permitir", "Autoriser", "Разрешить", "允许", "許可"],
    Deny => ["Rifiuta", "Deny", "Ablehnen", "Rechazar", "Refuser", "Отклонить", "拒绝", "拒否"],
    ErrLocked => ["troppe password sbagliate: riprova tra {wait}", "too many wrong passwords: try again in {wait}", "zu viele falsche Passwörter: erneut versuchen in {wait}", "demasiadas contraseñas incorrectas: vuelve a intentarlo en {wait}", "trop de mots de passe erronés : réessayez dans {wait}", "слишком много неверных паролей: повторите через {wait}", "密码错误次数过多：请在 {wait} 后重试", "パスワードの間違いが多すぎます。{wait} 後に再試行してください"],
    ErrDeclined => ["la persona davanti al PC non ha consentito l'accesso", "the person at that PC did not allow the connection", "die Person an diesem PC hat die Verbindung nicht zugelassen", "la persona frente a ese PC no permitió la conexión", "la personne devant ce PC n'a pas autorisé la connexion", "человек за тем ПК не разрешил подключение", "对方电脑前的人未允许连接", "相手のPCの前にいる人が接続を許可しませんでした"],
    BridgeTitle => ["Ponte cloud", "Cloud bridge", "Cloud-Brücke", "Puente en la nube", "Pont cloud", "Облачный мост", "云中继", "クラウドブリッジ"],
    BridgeHelp => [
        "Collega i PC anche quando il collegamento diretto non è possibile (CGNAT, router senza UPnP, reti aziendali). Serve un computer sempre acceso con indirizzo pubblico, ad esempio un piccolo server cloud, su cui gira \"magicremote bridge\". Il ponte passa solo dati cifrati: non vede schermo, tasti né password.",
        "Connects PCs even when a direct connection is not possible (carrier-grade NAT, routers without UPnP, office networks). It needs an always-on computer with a public address, such as a small cloud server, running \"magicremote bridge\". The bridge only passes encrypted data: it cannot see the screen, keys or passwords.",
        "Verbindet PCs auch dann, wenn keine direkte Verbindung möglich ist (CGNAT, Router ohne UPnP, Firmennetze). Dafür braucht es einen ständig laufenden Computer mit öffentlicher Adresse, etwa einen kleinen Cloud-Server, auf dem \"magicremote bridge\" läuft. Die Brücke leitet nur verschlüsselte Daten weiter: Bildschirm, Tasten und Passwörter sieht sie nicht.",
        "Conecta los PC incluso cuando no es posible una conexión directa (CGNAT, routers sin UPnP, redes de empresa). Necesita un ordenador siempre encendido con dirección pública, por ejemplo un pequeño servidor en la nube, que ejecute \"magicremote bridge\". El puente solo pasa datos cifrados: no ve la pantalla, las teclas ni las contraseñas.",
        "Relie les PC même quand une connexion directe est impossible (CGNAT, routeur sans UPnP, réseau d'entreprise). Il faut un ordinateur toujours allumé avec une adresse publique, par exemple un petit serveur cloud, qui exécute « magicremote bridge ». Le pont ne transmet que des données chiffrées : il ne voit ni l'écran, ni les touches, ni les mots de passe.",
        "Соединяет ПК, даже когда прямое подключение невозможно (CGNAT, роутер без UPnP, корпоративная сеть). Нужен постоянно включённый компьютер с публичным адресом, например небольшой облачный сервер, на котором запущен \"magicremote bridge\". Мост передаёт только зашифрованные данные: он не видит ни экран, ни клавиши, ни пароли.",
        "即使无法直接连接（运营商级 NAT、路由器不支持 UPnP、公司网络），也能连接电脑。需要一台一直开机、有公网地址的计算机（例如一台小型云服务器）运行 \"magicremote bridge\"。中继只转发加密数据：看不到屏幕、按键或密码。",
        "直接接続できない場合（CGNAT、UPnP非対応のルーター、社内ネットワーク）でもPCをつなぎます。公開アドレスを持つ常時稼働のコンピューター（小さなクラウドサーバーなど）で \"magicremote bridge\" を動かす必要があります。ブリッジは暗号化されたデータを中継するだけで、画面・キー入力・パスワードは見えません。",
    ],
    BridgeFp => ["Impronta del ponte: {fp}", "Bridge fingerprint: {fp}", "Fingerabdruck der Brücke: {fp}", "Huella del puente: {fp}", "Empreinte du pont : {fp}", "Отпечаток моста: {fp}", "中继指纹：{fp}", "ブリッジのフィンガープリント：{fp}"],
    BridgeEnabled => ["Ponte cloud attivo", "Cloud bridge on", "Cloud-Brücke aktiv", "Puente en la nube activado", "Pont cloud activé", "Облачный мост включён", "启用云中继", "クラウドブリッジを使う"],
    BridgeAddress => ["Indirizzo del ponte", "Bridge address", "Adresse der Brücke", "Dirección del puente", "Adresse du pont", "Адрес моста", "中继地址", "ブリッジのアドレス"],
    BridgeToken => ["Codice del ponte", "Bridge access code", "Zugangscode der Brücke", "Código del puente", "Code d'accès du pont", "Код доступа к мосту", "中继访问码", "ブリッジのアクセスコード"],
    BridgeTokenHint => ["se il ponte ne ha uno", "if the bridge has one", "falls die Brücke einen hat", "si el puente tiene uno", "si le pont en a un", "если он задан", "如果中继设置了", "設定されている場合"],
    BridgeSaved => [
        "Ponte cloud salvato. Se stai già condividendo questo PC, ferma e riavvia la condivisione.",
        "Cloud bridge saved. If you are already sharing this PC, stop and start sharing again.",
        "Cloud-Brücke gespeichert. Falls du diesen PC schon freigibst, beende die Freigabe und starte sie neu.",
        "Puente guardado. Si ya estás compartiendo este PC, detén y vuelve a iniciar el uso compartido.",
        "Pont cloud enregistré. Si vous partagez déjà ce PC, arrêtez puis relancez le partage.",
        "Облачный мост сохранён. Если доступ к этому ПК уже открыт, остановите и запустите его снова.",
        "云中继已保存。如果正在共享此电脑，请停止后重新开始共享。",
        "クラウドブリッジを保存しました。すでにこのPCを共有中なら、共有を停止して再開してください。",
    ],
    BridgeNeedAddr => ["Inserisci l'indirizzo del ponte.", "Enter the bridge address.", "Gib die Adresse der Brücke ein.", "Introduce la dirección del puente.", "Saisissez l'adresse du pont.", "Введите адрес моста.", "请输入中继地址。", "ブリッジのアドレスを入力してください。"],
    BridgeOnline => ["collegato a {addr}: raggiungibile da qualsiasi rete", "connected to {addr}: reachable from any network", "mit {addr} verbunden: aus jedem Netz erreichbar", "conectado a {addr}: accesible desde cualquier red", "connecté à {addr} : joignable depuis n'importe quel réseau", "подключён к {addr}: доступен из любой сети", "已连接 {addr}：可从任何网络访问", "{addr} に接続済み：どのネットワークからでも接続可能"],
    BridgeConnecting => ["collegamento a {addr}…", "connecting to {addr}…", "Verbindung zu {addr}…", "conectando con {addr}…", "connexion à {addr}…", "подключение к {addr}…", "正在连接 {addr}…", "{addr} に接続中…"],
    BridgeFailed => ["{addr} non raggiungibile ({e}); riprovo", "{addr} not reachable ({e}); retrying", "{addr} nicht erreichbar ({e}); neuer Versuch läuft", "{addr} no accesible ({e}); reintentando", "{addr} injoignable ({e}) ; nouvel essai", "{addr} недоступен ({e}); повторная попытка", "无法访问 {addr}（{e}）；正在重试", "{addr} に接続できません（{e}）。再試行中"],
    BridgeOffRow => ["spento (si attiva più sotto)", "off (turn it on below)", "aus (weiter unten einschalten)", "desactivado (actívalo más abajo)", "désactivé (activez-le plus bas)", "выключен (включите ниже)", "未启用（可在下方启用）", "オフ（下で有効にできます）"],
    StateOnBridge => ["acceso, tramite ponte cloud", "on, through the cloud bridge", "an, über die Cloud-Brücke", "encendido, a través del puente", "allumé, via le pont cloud", "включён, через облачный мост", "已开机（通过云中继）", "オン（クラウドブリッジ経由）"],
    ViaBridge => ["tramite ponte cloud", "through the cloud bridge", "über die Cloud-Brücke", "a través del puente", "via le pont cloud", "через облачный мост", "通过云中继", "クラウドブリッジ経由"],
    AddrHintBridge => ["vuoto = tramite ponte cloud", "empty = through the cloud bridge", "leer = über die Cloud-Brücke", "vacío = a través del puente", "vide = via le pont cloud", "пусто = через облачный мост", "留空 = 通过云中继", "空欄 = クラウドブリッジ経由"],
    ErrBridgeOffline => [
        "il PC non è collegato al ponte cloud (è acceso e sta condividendo con il ponte attivo?)",
        "that PC is not connected to the cloud bridge (is it on and sharing, with the bridge turned on?)",
        "dieser PC ist nicht mit der Cloud-Brücke verbunden (ist er an und freigegeben, mit aktiver Brücke?)",
        "ese PC no está conectado al puente (¿está encendido y compartiendo, con el puente activado?)",
        "ce PC n'est pas connecté au pont cloud (est-il allumé et partagé, avec le pont activé ?)",
        "этот ПК не подключён к облачному мосту (он включён, доступ открыт и мост включён?)",
        "该电脑未连接到云中继（是否已开机、正在共享并启用了云中继？）",
        "そのPCはクラウドブリッジに接続していません（電源が入り、ブリッジを有効にして共有中ですか？）",
    ],
    ErrBridgeCode => ["codice del ponte cloud sbagliato", "wrong cloud bridge access code", "falscher Zugangscode der Cloud-Brücke", "código del puente incorrecto", "code d'accès du pont incorrect", "неверный код доступа к облачному мосту", "云中继访问码错误", "クラウドブリッジのアクセスコードが違います"],
    ErrBridgeDown => ["il ponte cloud non risponde", "the cloud bridge does not answer", "die Cloud-Brücke antwortet nicht", "el puente en la nube no responde", "le pont cloud ne répond pas", "облачный мост не отвечает", "云中继没有响应", "クラウドブリッジが応答しません"],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_text_in_every_language() {
        for key in K::ALL {
            for lang in Lang::ALL {
                assert!(!t(lang, *key).is_empty(), "{key:?} missing in {lang:?}");
            }
            // Placeholders must be the same in all languages.
            let marks = |s: &str| s.matches('{').count();
            let it = marks(t(Lang::It, *key));
            for lang in Lang::ALL {
                assert_eq!(
                    marks(t(lang, *key)),
                    it,
                    "{key:?} placeholders differ in {lang:?}"
                );
            }
        }
    }

    #[test]
    fn locale_codes() {
        assert_eq!(Lang::from_code("it-IT"), Some(Lang::It));
        assert_eq!(Lang::from_code("zh_CN.UTF-8"), Some(Lang::Zh));
        assert_eq!(Lang::from_code("ja"), Some(Lang::Ja));
        assert_eq!(Lang::from_code("pt-BR"), None);
        assert_eq!(tf(Lang::En, K::IsOn, &[("name", "casa")]), "casa is on");
    }
}
