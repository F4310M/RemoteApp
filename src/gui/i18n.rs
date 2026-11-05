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
