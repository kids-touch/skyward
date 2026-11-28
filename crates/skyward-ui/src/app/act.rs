//! What a control does once it is tapped, typed on or swiped.

use super::*;

impl App {
    /// The runtime read the slot's vbmeta flags: the switches take them.
    pub fn set_avb(&mut self, status: Result<AvbStatus, String>) {
        if let Ok(status) = &status {
            self.state.avb_verity = status.verity;
            self.state.avb_verification = status.verification;
        }
        self.state.avb = Some(status);
        self.refresh();
    }

    /// A write of the vbmeta flags ended: the page says how, and reads the
    /// flags again where it went.
    pub fn avb_changed(&mut self, result: Result<AvbStatus, String>, now: u64) {
        match &result {
            Ok(_) => self.state.avb_notice = Some(self.state.lang.pick(texts::local::AVB_WRITTEN).to_string()),
            Err(why) => self.state.avb_notice = Some(why.clone()),
        }
        self.set_avb(result);
        self.wake(now);
    }

    /// Carry out a tapped control.
    pub(super) fn act(&mut self, action: Action, now: u64) -> Vec<Effect> {
        let l = self.state.lang;
        if self.state.new_folder_closing
            && matches!(action, Action::Type(_) | Action::MoveCaret(_) | Action::NewFolderConfirm)
        {
            return Vec::new();
        }
        match action {
            Action::Open(s) => {
                if s == Screen::Avb {
                    self.state.avb = None;
                    self.state.avb_notice = None;
                }
                if s == Screen::KernelSu {
                    self.state.kernel_su = None;
                    self.state.kernel_su_job = None;
                }
                if s == Screen::Magisk {
                    self.state.magisk = None;
                    self.state.magisk_job = None;
                }
                if s == Screen::Modules {
                    self.state.modules = None;
                    self.state.module_at = None;
                }
                if s == Screen::Search && self.screen() == Screen::Home {
                    // The bar moves up from where it stands unfolded: a
                    // page easing there for the keys gets there at once.
                    if let Some((rest, _)) = self.seek.take() {
                        self.top_mut().1 = rest;
                    }
                    self.start_morph(true, now);
                    self.push(s);
                } else {
                    self.open(s);
                }
                let mut fx = self.arrive_at(s);
                if s == Screen::Avb {
                    fx.push(Effect::ReadAvb);
                }
                return fx;
            }
            Action::OpenData => return self.open_data(),
            Action::Skip => {
                self.leave(true);
                if self.stack.len() > 1 {
                    self.stack.pop();
                } else {
                    *self.top_mut() = (Screen::Home, 0.0, Focus::Unset);
                }
                self.settle_page();
                self.refresh();
            }
            Action::Back | Action::CloseSearch => return self.back(now),
            Action::Nothing => {}
            Action::Theme { dark } => {
                if self.state.dark != dark {
                    self.state.dark = dark;
                    self.refresh();
                }
            }
            // The switch moves at once; the runtime then tells how the WLAN
            // came up, or went off.
            Action::Toggle(Switch::Wlan) => {
                let w = self.state.wlan.as_mut().expect("the WLAN switch shows on a phone whose WLAN is brought up");
                let on = matches!(w.power, Power::Off | Power::Failed(_));
                *w = Wlan { power: if on { Power::Starting } else { Power::Off }, ..Wlan::default() };
                self.refresh();
                return vec![Effect::Wlan(on)];
            }
            // The zone the system's gave the clock stays as the one chosen,
            // where it is one the pages list.
            Action::Toggle(Switch::SystemTimeZone) => {
                if self.state.follow_system_zone
                    && let Some(at) = self.state.zone_listed()
                {
                    self.state.time_zone = at;
                }
                self.state.follow_system_zone.toggle();
                self.refresh();
            }
            Action::Toggle(switch) => {
                if matches!(switch, Switch::AvbVerity | Switch::AvbVerification) {
                    let on = match switch {
                        Switch::AvbVerity => self.state.avb_verity,
                        Switch::AvbVerification => self.state.avb_verification,
                        _ => false,
                    };
                    // Turning a check back on restores the flag bytes the
                    // bootloader signed; turning one off is what leaves this
                    // phone at the bootloader.
                    let blocked = on
                        && (!self.state.avb_writable
                            || (switch == Switch::AvbVerification && !self.state.avb_verification_writable));
                    if blocked {
                        let why = if self.state.avb_writable { texts::local::AVB_VERITY_ONLY } else { texts::local::AVB_UNSUPPORTED };
                        self.say(l.pick(why), now);
                        return Vec::new();
                    }
                }
                let on = match switch {
                    Switch::Vibrate => &mut self.state.vibrate,
                    Switch::FileTransfer => &mut self.state.file_transfer,
                    Switch::FileServe => &mut self.state.file_serve,
                    Switch::MergeAfter => &mut self.state.merge_after,
                    Switch::Compress => &mut self.state.compress,
                    Switch::Verify => &mut self.state.verify,
                    Switch::Signature => &mut self.state.check_signature,
                    Switch::KeepRecovery => &mut self.state.keep_recovery,
                    Switch::ShowPassword => &mut self.state.show_password,
                    Switch::AodTap => &mut self.state.aod_tap,
                    Switch::WakeTap => &mut self.state.wake_tap,
                    Switch::AvbVerity => &mut self.state.avb_verity,
                    Switch::AvbVerification => &mut self.state.avb_verification,
                    Switch::KernelSuOnline => &mut self.state.kernel_su_online,
                    Switch::RememberData => &mut self.state.remember_data,
                    Switch::AutoWlan => &mut self.state.auto_wlan,
                    Switch::Wlan | Switch::SystemTimeZone => unreachable!("the switch is turned above"),
                };
                *on = !*on;
                let now_on = *on;
                if matches!(switch, Switch::AodTap | Switch::WakeTap) {
                    self.dark_touch = None;
                    self.last_tap = None;
                }
                self.refresh();
                if switch == Switch::FileTransfer {
                    return vec![Effect::FileTransfer(now_on)];
                }
                if switch == Switch::AutoWlan {
                    return vec![Effect::WlanAuto { enabled: now_on, saved: self.state.saved_wlan.clone() }];
                }
            }
            Action::MountToggle(i) => {
                if self.state.mount_failure.take().is_some() {
                    self.refresh();
                }
                let row = &self.state.mounts.as_ref().expect("a partition toggled is one the page lists")[i];
                let base = row.base.clone();
                return vec![if row.mounted { Effect::Unmount(base) } else { Effect::Mount(base) }];
            }
            Action::RebootTarget(i) => {
                self.state.reboot = i;
                self.refresh();
            }
            // Everything takes in every part, so choosing it leaves the
            // parts out, and choosing a part on its own leaves it out.
            Action::WipeItem(item) => {
                let k = WIPES.iter().position(|&w| w == item).expect("the page offers only what it lists");
                let on = !self.state.wipe[k];
                match (on, item) {
                    (true, Wipe::All) => {
                        self.state.wipe = [false; WIPES.len()];
                        self.state.wipe[k] = true;
                    }
                    (true, _) => {
                        self.state.wipe[0] = false;
                        self.state.wipe[k] = true;
                    }
                    (false, _) => self.state.wipe[k] = false,
                }
                self.refresh();
            }
            Action::Confirm(_) => self.say(l.pick(texts::toast::SLIDE_RIGHT), now),
            Action::Slider(_) | Action::Pattern => {}
            Action::Sideload => match self.state.install {
                Install::Idle | Install::Finished { .. } => {
                    self.begin_install(Install::Starting, Source::Computer, None);
                    if self.screen() == Screen::Installing {
                        self.refresh();
                    } else {
                        self.open(Screen::Installing);
                    }
                    return vec![Effect::Sideload];
                }
                _ => self.say(l.pick(INSTALL_UNFINISHED), now),
            },
            Action::FlashPartition(k) => {
                if let Install::Choosing(choice) = &mut self.state.install {
                    choice.partition = k;
                    self.refresh();
                }
            }
            Action::FlashSlots(pick) => {
                if let Install::Choosing(choice) = &mut self.state.install {
                    choice.pick = pick;
                    self.refresh();
                }
            }
            Action::SystemSize(k) => {
                if let Install::SystemChoosing(choice) = &mut self.state.install {
                    choice.size = k;
                    self.refresh();
                }
            }
            Action::PickSystem(who) => {
                if self.state.systems_work.is_none() {
                    self.state.system_pick = Some(who);
                    self.state.asking_delete = false;
                    self.state.confirm = 0.0;
                    self.refresh();
                }
            }
            Action::RebootTo(target) => return vec![Effect::Reboot(target)],
            Action::EnterRecovery => return vec![Effect::EnterRecovery],
            Action::Dismiss => {
                self.stack = vec![(Screen::Home, 0.0, Focus::Unset)];
                self.settle_page();
                self.refresh();
                return vec![Effect::Dismiss];
            }
            Action::PickEntry(op) => {
                let path = self.entry_path();
                self.state.clip = Some(Clip { op, path });
                return self.back(now);
            }
            // A folder cannot go inside itself, and an entry moved to the
            // folder it is in stays put: the entry stays picked for another
            // folder.
            Action::Paste => {
                let clip = self.state.clip.clone().expect("only a picked entry is pasted");
                let (path, folder) = (std::path::Path::new(&clip.path), std::path::Path::new(&self.state.files.path));
                let name = path.file_name().map_or_else(|| clip.path.clone(), |n| n.to_string_lossy().into_owned());
                if folder.starts_with(path) {
                    self.say(&texts::toast::inside_itself(l, name), now);
                } else if clip.op == FileOp::Move && path.parent() == Some(folder) {
                    self.say(&texts::toast::already_there(l, name), now);
                } else {
                    return vec![Effect::Paste { op: clip.op, path: clip.path, into: self.state.files.path.clone(), clash: None }];
                }
            }
            Action::Unpick => {
                self.state.clip = None;
                self.refresh();
            }
            Action::RenameEntry => {
                self.state.new_name = self.state.file.as_ref().expect("the file page shows an entry").name.clone();
                self.state.layer = Layer::Letters;
                self.open(Screen::Rename);
            }
            Action::EditEntry => return vec![Effect::OpenText(self.entry_path())],
            Action::InstallEntry => {
                let size = self.state.file.as_ref().expect("the file page shows an entry").size;
                return self.install_opened(self.entry_path(), size, now);
            }
            Action::InstallAsModule => return vec![Effect::ModuleInstall { path: self.entry_path() }],
            Action::SaveText => {
                let ed = self.state.editor.as_ref().expect("the editor page shows a file");
                return vec![Effect::SaveText { path: ed.path.clone(), text: ed.text() }];
            }
            Action::OpenPermissions => self.open(Screen::Permissions),
            Action::ModeBit(bit) => {
                let mode = self.state.file.as_ref().expect("the permissions page shows an entry").mode;
                return vec![Effect::SetMode { path: self.entry_path(), mode: (mode ^ bit) & 0o7777 }];
            }
            Action::AskDelete => {
                self.state.asking_delete = true;
                self.state.confirm = 0.0;
                self.refresh();
            }
            Action::Resolve(choice) => {
                let (clip, into, _) = self.state.clash.take().expect("a choice answers a clash");
                self.leave(true);
                self.stack.pop();
                self.settle_page();
                self.refresh();
                // Skipping the one entry picked leaves nothing to do.
                if choice == ClashChoice::Skip {
                    self.state.clip = None;
                    return Vec::new();
                }
                return vec![Effect::Paste { op: clip.op, path: clip.path, into, clash: Some(choice) }];
            }
            Action::StopJob => {
                let job = self.state.job.as_mut().expect("only work under way is stopped");
                if job.outcome.is_none() && !job.stopping {
                    job.stopping = true;
                    self.refresh();
                    return vec![Effect::StopJob];
                }
            }
            Action::StopRelabel => {
                let job = self.state.relabel.as_mut().expect("only work under way is stopped");
                if job.outcome.is_none() && !job.stopping {
                    job.stopping = true;
                    self.refresh();
                    return vec![Effect::StopRelabel];
                }
            }
            Action::OpenFilesystem(k) => {
                self.state.fs_chosen = k;
                self.state.fs_action = FsAction::Repair;
                self.open(Screen::Filesystem);
            }
            Action::ChooseFs(choice) => {
                let row = self.fs_row().expect("the page of a partition shows one that was read");
                let action = match choice {
                    FsChoice::Repair => FsAction::Repair,
                    FsChoice::Remake(k) => FsAction::Remake(row.others[k].clone()),
                };
                self.state.fs_action = action;
                self.refresh();
            }
            Action::ScreenOff(k) => {
                self.state.screen_off = k;
                self.refresh();
            }
            Action::Hour12(on) => {
                if self.state.hour12 != on {
                    self.state.hour12 = on;
                    self.refresh();
                }
            }
            Action::AodDate(k) => {
                if k < AOD_DATES.len() && self.state.aod_date != k {
                    self.state.aod_date = k;
                    self.refresh();
                }
            }
            Action::SortKey(k) => {
                if let Some(key) = SORT_KEYS.get(k).copied()
                    && self.state.sort != key
                {
                    self.set_sort(key, self.state.sort_desc);
                }
            }
            Action::SortOrder(desc) => {
                if self.state.sort_desc != desc {
                    self.set_sort(self.state.sort, desc);
                }
            }
            Action::FilesSearch => {
                if self.screen() == Screen::Files {
                    self.state.files_query.clear();
                    self.state.files.set_query("");
                    self.state.files_search = true;
                    self.open(Screen::FileSearch);
                }
            }
            Action::NewFolder => {
                if self.screen() == Screen::Files {
                    self.state.new_name = l.pick(texts::files::NEW_FOLDER).to_string();
                    self.state.new_folder_cursor = self.state.new_name.len();
                    self.state.layer = Layer::Letters;
                    self.state.new_folder = true;
                    self.state.new_folder_closing = false;
                    self.state.new_folder_default = true;
                    self.state.new_folder_rise = 1000.0;
                    self.folder_open = Some(Spring::new(now, 1000.0, 0.0, 0.26));
                    self.folder_close = None;
                    self.fling = None;
                    self.bounce = None;
                    self.seek = None;
                    self.pull = 0.0;
                    self.refresh();
                }
            }
            Action::NewFolderConfirm => return self.make_folder(now),
            Action::MoveCaret(at) => {
                if self.state.new_folder && !self.state.new_folder_closing {
                    let mut at = at.min(self.state.new_name.len());
                    while at > 0 && !self.state.new_name.is_char_boundary(at) {
                        at -= 1;
                    }
                    self.state.new_folder_default = false;
                    self.state.new_folder_cursor = at;
                    self.edits += 1;
                    self.refresh();
                }
            }
            Action::OpenPlace(i) => {
                let mut places = vec!["/sdcard"];
                if self.state.usb_drive.is_some() {
                    places.push(USB_DRIVE);
                }
                places.push("/");
                if let Some(path) = places.get(i) {
                    self.state.files_query.clear();
                    self.state.files_search = false;
                    self.state.new_folder = false;
                    self.state.new_folder_closing = false;
                    self.state.new_folder_default = false;
                    self.state.new_folder_cursor = 0;
                    self.state.new_name.clear();
                    self.state.new_folder_rise = 0.0;
                    self.folder_open = None;
                    self.folder_close = None;
                    self.leave(true);
                    self.stack.pop();
                    self.settle_page();
                    return vec![Effect::ReadDir((*path).into())];
                }
            }
            Action::CloseFilesSearch => {
                if self.state.files_search {
                    self.state.files_search = false;
                    self.refresh();
                }
            }
            Action::ClearFilesSearch => {
                self.state.files_query.clear();
                self.state.files.set_query("");
                self.refresh();
            }
            Action::TimeZone(k) => {
                self.state.time_zone = k;
                self.state.follow_system_zone = false;
                self.refresh();
            }
            Action::Language(l) => {
                self.state.lang = l;
                self.state.language_chosen = true;
                self.refresh();
            }
            Action::BackupPart(k) => {
                self.state.backup[k].toggle();
                self.refresh();
            }
            Action::BackupTo(place) => {
                self.state.backup_to = place;
                self.refresh();
            }
            // All a backup holds is ticked to restore at first.
            Action::OpenBackup(i) => {
                let entry = self.state.backups[i].clone();
                let parts = entry.held.as_ref().map_or([false; 5], |h| h.parts);
                self.state.restore = Some((entry, parts));
                self.state.asking_delete = false;
                self.open(Screen::RestoreBackup);
            }
            Action::RestorePart(k) => {
                let (_, parts) = self.state.restore.as_mut().expect("the restore page shows a backup");
                parts[k].toggle();
                self.refresh();
            }
            Action::RescueKind(k) => {
                if let Some(on) = self.state.rescue.kinds.get_mut(k) { *on = !*on; }
                self.refresh();
            }
            Action::StartRescueScan => {
                if !self.state.data.storage_open() {
                    self.say(l.pick(DECRYPT_FIRST), now);
                } else if !self.state.rescue.kinds.contains(&true) {
                    self.say(if l == Lang::Hans { "请至少选择一种数据类型" } else { "Choose at least one kind of data" }, now);
                } else {
                    return vec![Effect::RescueScan(self.state.rescue.kinds)];
                }
            }
            Action::StartRescueExport => {
                if !self.state.data.storage_open() {
                    self.say(l.pick(DECRYPT_FIRST), now);
                } else if self.state.rescue.found == 0 {
                    self.say(if l == Lang::Hans { "请先扫描数据" } else { "Scan for data first" }, now);
                } else {
                    return vec![Effect::RescueExport];
                }
            }
            Action::StopRescue => {
                if self.state.rescue.running && !self.state.rescue.stopping {
                    self.state.rescue.stopping = true;
                    self.refresh();
                    return vec![Effect::StopRescue];
                }
            }
            Action::StopBackup => {
                let job = self.state.backup_job.as_mut().expect("only work under way is stopped");
                if job.outcome.is_none() && !job.stopping {
                    job.stopping = true;
                    self.refresh();
                    return vec![Effect::StopBackup];
                }
            }
            Action::LogPart(k) => {
                self.state.logs[k].toggle();
                self.refresh();
            }
            Action::LogsTo(k) => {
                self.state.logs_to = k;
                self.refresh();
            }
            Action::ExportLogs => {
                let ([recovery, kernel], storage) = (self.state.logs, self.state.logs_to == 1);
                if !recovery && !kernel {
                    self.say(l.pick(texts::toast::CHOOSE_LOGS), now);
                } else if storage && !self.state.data.storage_open() {
                    self.say(l.pick(DECRYPT_FIRST), now);
                } else {
                    return vec![Effect::ExportLogs { recovery, kernel, storage }];
                }
            }
            Action::PickSlot(slot) => {
                self.state.next_slot = Some(slot);
                self.refresh();
            }
            Action::Keyboard(shown) => self.slide_keys(!shown, now),
            Action::Type(cap) if self.screen() == Screen::Terminal => return self.term_typed(cap),
            Action::Type(cap) if self.screen() == Screen::Editor => self.edit_typed(cap),
            Action::Type(cap) => return self.type_key(cap, now),
            Action::TermKey(key) if self.screen() == Screen::Editor => {
                let ed = self.state.editor.as_mut().expect("the editor page shows a file");
                match key {
                    TermKey::Tab => ed.insert('\t'),
                    TermKey::Left => ed.left(),
                    TermKey::Up => ed.up(),
                    TermKey::Down => ed.down(),
                    TermKey::Right => ed.right(),
                    TermKey::Esc | TermKey::Ctrl => unreachable!("the editor's row has no {key:?}"),
                }
                self.edits += 1;
                self.refresh_text();
            }
            Action::TermKey(key) => {
                let Some(term) = self.term.as_mut() else { return Vec::new() };
                let bytes = term.key(key);
                let running = term.running;
                self.refresh();
                if running && !bytes.is_empty() {
                    return vec![Effect::TermInput(bytes)];
                }
            }
            Action::ClearSearch => {
                self.state.search.clear();
                self.edits += 1;
                self.refresh();
            }
            Action::OpenEntry(i) => {
                let entry = self.state.files.entry(i).clone();
                match entry.kind {
                    FileKind::Folder { .. } | FileKind::Link { folder: true, .. } => {
                        let scroll = self.top().1;
                        self.state.files.leave(scroll);
                        if self.screen() == Screen::FileSearch {
                            self.leave(true);
                            self.stack.pop();
                            self.settle_page();
                        }
                        return vec![Effect::ReadDir(self.state.files.inside(&entry.name))];
                    }
                    FileKind::File if matches!(files::file_type(&entry.name), FileType::Text | FileType::Code) => {
                        return vec![Effect::OpenText(self.state.files.inside(&entry.name))];
                    }
                    _ if installs(&entry) => {
                        let path = self.state.files.inside(&entry.name);
                        return self.install_opened(path, entry.size, now);
                    }
                    FileKind::File | FileKind::Link { folder: false, .. } | FileKind::Special => {
                        self.state.file = Some(entry);
                        self.open(Screen::FileInfo);
                    }
                }
            }
            // A shell already running goes to the folder; otherwise one starts
            // there.
            Action::TerminalHere => {
                let folder = self.state.files.path.clone();
                let running = self.term.as_ref().is_some_and(|term| term.running);
                self.open(Screen::Terminal);
                self.state.layer = Layer::Letters;
                if running {
                    return vec![Effect::ShellTo(folder)];
                }
                self.shell_folder = folder.clone();
                return vec![Effect::StartShell(folder)];
            }
            Action::ChooseEntry(i) => {
                let screen = self.screen();
                let Screen::Choose(place) = screen else { unreachable!("only a place's folder is chosen in") };
                let folder = self.state.folder(screen);
                let entry = folder.entry(i).clone();
                let path = folder.inside(&entry.name);
                match entry.kind {
                    FileKind::Folder { .. } | FileKind::Link { folder: true, .. } => {
                        let scroll = self.top().1;
                        self.state.folder_mut(screen).leave(scroll);
                        return vec![Effect::ReadPlace(place, path)];
                    }
                    FileKind::File | FileKind::Link { folder: false, .. } => return self.install_file(place, path, entry.size, now),
                    FileKind::Special => unreachable!("a place lists no special files"),
                }
            }
            Action::ChooseKoEntry(i) => {
                let screen = self.screen();
                let Screen::ChooseKo(place) = screen else { unreachable!("only a place's folder is chosen into a module") };
                let folder = self.state.folder(screen);
                let entry = folder.entry(i).clone();
                let path = folder.inside(&entry.name);
                match entry.kind {
                    FileKind::Folder { .. } | FileKind::Link { folder: true, .. } => {
                        let scroll = self.top().1;
                        self.state.folder_mut(screen).leave(scroll);
                        return vec![Effect::ReadKoPlace(place, path)];
                    }
                    FileKind::File | FileKind::Link { folder: false, .. } => {
                        if std::path::Path::new(&entry.name).extension().is_some_and(|ext| ext.eq_ignore_ascii_case("ko")) {
                            self.state.kernel_su_ko = Some(path);
                            return self.back(now);
                        }
                        self.say(l.pick(texts::kernel_su::NOT_KO), now);
                    }
                    FileKind::Special => unreachable!("a place lists no special files"),
                }
            }
            Action::OpenAncestor(depth) => {
                let screen = self.screen();
                if let Some(path) = self.state.folder(screen).ancestor(depth) {
                    return vec![match screen {
                        Screen::Choose(place) => Effect::ReadPlace(place, path),
                        Screen::ChooseKo(place) => Effect::ReadKoPlace(place, path),
                        _ => Effect::ReadDir(path),
                    }];
                }
            }
            Action::JoinNetwork(i) => {
                let w = self.state.wlan.as_ref().expect("networks show on a phone whose WLAN is brought up");
                // A scan may have changed the list since the page was drawn.
                let Some(network) = w.networks.get(i).cloned() else { return Vec::new() };
                if w.joined.as_ref().is_some_and(|j| j.ssid == network.ssid) {
                    return self.act(Action::OpenJoined, now);
                }
                match network.security {
                    Security::Wep => self.say(l.pick(texts::toast::WEP_UNSUPPORTED), now),
                    Security::Enterprise => self.say(l.pick(texts::toast::ENTERPRISE_UNSUPPORTED), now),
                    s if s.password() => self.ask_password(network),
                    _ => {
                        return vec![Effect::JoinWlan {
                            ssid: network.ssid,
                            name: network.name,
                            security: network.security,
                            password: Secret::default(),
                        }]
                    }
                }
            }
            Action::JoinSavedNetwork(i) => {
                let Some(saved) = self.state.saved_wlan.get(i).cloned() else { return Vec::new() };
                if self.state.wlan.as_ref().and_then(|w| w.joined.as_ref()).is_some_and(|j| j.ssid == saved.ssid) {
                    return self.act(Action::OpenJoined, now);
                }
                if !saved.security.joinable() {
                    self.say(l.pick(texts::toast::WEP_UNSUPPORTED), now);
                    return Vec::new();
                }
                return vec![Effect::JoinWlan {
                    ssid: saved.ssid,
                    name: saved.name,
                    security: saved.security,
                    password: saved.password,
                }];
            }
            Action::NetworkDetails(i) => {
                let w = self.state.wlan.as_ref().expect("networks show on a phone whose WLAN is brought up");
                let Some(network) = w.networks.get(i).cloned() else { return Vec::new() };
                self.state.details = Some(network);
                self.open(Screen::WlanNetwork);
            }
            Action::OpenJoined => {
                let w = self.state.wlan.as_ref().expect("a network is chosen on a phone whose WLAN is brought up");
                let Some(j) = w.joined.clone() else { return Vec::new() };
                // As found, where it still is; as it was chosen otherwise.
                let found = w.networks.iter().find(|n| n.ssid == j.ssid).cloned();
                let network = found.unwrap_or(Network {
                    ssid: j.ssid,
                    name: j.name,
                    signal: 0,
                    frequency: 0,
                    bands: Default::default(),
                    security: j.security,
                });
                if j.stage == Joining::WrongPassword {
                    self.ask_password(network);
                } else {
                    self.state.details = Some(network);
                    self.open(Screen::WlanNetwork);
                }
            }
            Action::LeaveNetwork => {
                let mut fx = self.back(now);
                fx.push(Effect::LeaveWlan);
                return fx;
            }
            Action::Rescan => return vec![Effect::ScanWlan],
            Action::AddNetwork => {
                self.state.adding = Some(Adding { name: String::new(), security: Security::Open, field: Field::Name, choosing: false });
                self.state.password.clear();
                self.state.show_password = false;
                self.state.layer = Layer::Letters;
                self.open(Screen::WlanAdd);
            }
            Action::TypeInto(field) => {
                let a = self.state.adding.as_mut().expect("the fields are on the page that adds a network");
                if a.field != field {
                    a.field = field;
                    self.refresh();
                }
            }
            Action::ChooseSecurity => {
                self.state.adding.as_mut().expect("the protection is on the page that adds a network").choosing = true;
                // The page draws the row held down while its list is open.
                self.row_press = None;
                self.row_art = None;
                self.refresh();
            }
            Action::PickSecurity(security) => {
                let a = self.state.adding.as_mut().expect("the protections open on the page that adds a network");
                a.security = security;
                a.choosing = false;
                // With no password to type, the keyboard types the name.
                if !security.password() {
                    a.field = Field::Name;
                }
                self.refresh();
            }
            Action::CloseChoices => {
                self.state.adding.as_mut().expect("the protections open on the page that adds a network").choosing = false;
                self.refresh();
            }
            // Asking takes a network, which the WLAN page joins.
            Action::CheckUpdate => {
                if !self.state.online() {
                    self.open(Screen::Wlan);
                    return self.arrive_at(Screen::Wlan);
                }
                if self.state.check != Check::Asking {
                    self.state.check = Check::Asking;
                    self.refresh();
                    return vec![Effect::CheckUpdate];
                }
            }
            // The package goes to internal storage, which the decrypt page
            // opens, over a network the WLAN page joins.
            Action::Download => {
                let release = self.state.newest().expect("the download is offered for the newest system").clone();
                if !self.state.data.storage_open() {
                    self.open(Screen::Decrypt);
                    return self.arrive_at(Screen::Decrypt);
                }
                if !self.state.online() {
                    self.open(Screen::Wlan);
                    return self.arrive_at(Screen::Wlan);
                }
                if !self.state.downloading() {
                    self.state.download = Some(DownloadJob { progress: None, rate: 0, outcome: None, stopping: false });
                    self.refresh();
                    return vec![Effect::Download(release)];
                }
            }
            Action::StopDownload => {
                let job = self.state.download.as_mut().expect("only a download under way is stopped");
                if job.outcome.is_none() && !job.stopping {
                    job.stopping = true;
                    self.refresh();
                    return vec![Effect::StopDownload];
                }
            }
            Action::InstallDownloaded => {
                let name = self.state.newest().expect("the package installed is the newest system's").name.clone();
                let Have::Whole(size) = self.state.have else { unreachable!("only a whole package is offered to install") };
                return self.install_file(Place::Storage, format!("{DOWNLOADS}/{name}"), size, now);
            }
            // Asking takes a network, which the WLAN page joins.
            Action::CheckSkyward => {
                if !self.state.online() {
                    self.open(Screen::Wlan);
                    return self.arrive_at(Screen::Wlan);
                }
                if self.state.skyward_check != SkywardCheck::Asking {
                    self.state.skyward_check = SkywardCheck::Asking;
                    self.refresh();
                    return vec![Effect::CheckSkyward];
                }
            }
            Action::SkywardDownloadSource(source) => {
                if self.state.skyward_source != source && !self.state.updating_skyward() {
                    self.state.skyward_source = source;
                    self.refresh();
                }
            }
            // The build goes into the recovery partitions an install may
            // write too, so one waits for the other.
            Action::UpdateSkyward => {
                let build = self.state.skyward_newest().and_then(|n| n.build.clone()).expect("the update is offered for the newest release's build");
                if !self.state.online() {
                    self.open(Screen::Wlan);
                    return self.arrive_at(Screen::Wlan);
                }
                if !matches!(self.state.install, Install::Idle | Install::Finished { .. }) {
                    self.say(l.pick(INSTALL_UNFINISHED), now);
                    return Vec::new();
                }
                if !self.state.updating_skyward() {
                    self.state.skyward_job = Some(SkywardJob { stage: None, rate: 0, outcome: None, stopping: false });
                    self.refresh();
                    return vec![Effect::UpdateSkyward { build, source: self.state.skyward_source }];
                }
            }
            // Only the download stops: writing, once begun, goes on to its
            // end.
            Action::StopSkywardUpdate => {
                let job = self.state.skyward_job.as_mut().expect("only an update under way is stopped");
                if job.outcome.is_none() && !job.stopping && !matches!(job.stage, Some(SkywardStage::Write { .. })) {
                    job.stopping = true;
                    self.refresh();
                    return vec![Effect::StopSkywardUpdate];
                }
            }
            Action::KernelSuKmi(at) => {
                self.set_kernel_su_kmi(at);
                return self.back(now);
            }
            Action::KernelSuModuleBuiltIn => {
                self.state.kernel_su_ko = None;
                self.refresh();
            }
            Action::RestoreKernelSu => {
                if self.state.data != Data::Open {
                    self.say(l.pick(texts::kernel_su::NO_DATA), now);
                } else if self.state.kernel_su_job.as_ref().is_some_and(|j| j.outcome.is_none()) {
                    self.say(l.pick(texts::kernel_su::BUSY), now);
                } else {
                    self.kernel_su_started();
                    return vec![Effect::RestoreKernelSu];
                }
            }
            Action::RestoreMagisk => {
                if self.state.data != Data::Open {
                    self.say(l.pick(texts::kernel_su::NO_DATA), now);
                } else if self.state.magisk_job.as_ref().is_some_and(|j| j.outcome.is_none()) {
                    self.say(l.pick(texts::kernel_su::BUSY), now);
                } else {
                    self.magisk_started();
                    return vec![Effect::RestoreMagisk];
                }
            }
            Action::ModuleOpen(at) => {
                if self.state.modules.as_ref().is_some_and(|r| r.as_ref().is_ok_and(|list| at < list.len())) {
                    self.open_module(at);
                    self.open(Screen::ModuleInfo);
                    return self.arrive_at(Screen::ModuleInfo);
                }
            }
            Action::ModuleToggle(at) => {
                if let Some(Ok(list)) = &self.state.modules
                    && let Some(module) = list.get(at).filter(|module| module.editable)
                {
                    return vec![Effect::ModuleSwitch { id: module.id.clone(), on: !module.enabled }];
                }
            }
            Action::ModuleRemove(at) => {
                if let Some(Ok(list)) = &self.state.modules
                    && let Some(module) = list.get(at).filter(|module| module.editable)
                {
                    return vec![Effect::ModuleRemove { id: module.id.clone(), on: !module.removing }];
                }
            }
            Action::SetAutoRoot(root) => {
                self.state.auto_root = if self.state.auto_root == root { AutoRoot::None } else { root };
                self.refresh();
            }
            Action::Unavailable(text) => self.say(text, now),
        }
        Vec::new()
    }

    /// Put every folder the file manager shows in the same order.
    fn set_sort(&mut self, key: SortKey, desc: bool) {
        self.state.sort = key;
        self.state.sort_desc = desc;
        for screen in [
            Screen::Files,
            Screen::Choose(Place::Storage),
            Screen::Choose(Place::UsbDrive),
            Screen::ChooseKo(Place::Storage),
            Screen::ChooseKo(Place::UsbDrive),
        ] {
            self.state.folder_mut(screen).sort(key, desc);
        }
        self.refresh();
    }

    /// Make the folder the new-folder dialog names, accepting `/` for a
    /// path below the folder shown or an absolute one.
    fn make_folder(&mut self, now: u64) -> Vec<Effect> {
        let l = self.state.lang;
        let name = self.state.new_name.trim_end_matches('/').to_string();
        let probe = name.trim_start_matches('/');
        if probe.is_empty() {
            self.say(l.pick(texts::toast::ENTER_NAME), now);
            return Vec::new();
        }
        let valid = std::path::Path::new(probe)
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)));
        if !valid {
            self.say(l.pick(texts::files::NEW_FOLDER_NOTE), now);
            return Vec::new();
        }
        let taken = !name.contains('/')
            && self
                .state
                .files
                .entries
                .as_ref()
                .is_ok_and(|entries| entries.iter().any(|e| e.name == name));
        if taken {
            self.say_taken(now);
            return Vec::new();
        }
        vec![Effect::Mkdir { path: self.state.files.path.clone(), name }]
    }

    /// The text the keyboard types into on this page.
    fn typing(&mut self) -> &mut String {
        if self.state.new_folder {
            return &mut self.state.new_name;
        }
        match self.screen() {
            Screen::Search => &mut self.state.search,
            Screen::Files => &mut self.state.files_query,
            Screen::Decrypt => &mut self.state.code,
            Screen::Rename => &mut self.state.new_name,
            Screen::FileSearch => &mut self.state.files_query,
            Screen::WlanPassword => &mut self.state.password,
            Screen::WlanAdd => {
                let a = self.state.adding.as_mut().expect("the page adds a network");
                match a.field {
                    Field::Name => &mut a.name,
                    Field::Password => &mut self.state.password,
                }
            }
            s => panic!("{s:?} has no text field"),
        }
    }

    /// A key of the on-screen keyboard. A capital lasts one letter; return
    /// opens the first result of a search, and runs a terminal's command.
    fn type_key(&mut self, cap: Cap, now: u64) -> Vec<Effect> {
        let (l, layer) = (self.state.lang, self.state.layer);
        if self.state.new_folder
            && self.state.new_folder_default
            && matches!(cap, Cap::Char(_) | Cap::Space | Cap::Backspace)
        {
            self.state.new_name.clear();
            self.state.new_folder_cursor = 0;
            self.state.new_folder_default = false;
        }
        match cap {
            Cap::Char(ch) => {
                let shifted = self.state.layer == Layer::Shifted;
                if self.state.new_folder {
                    let ch = if shifted { ch.to_ascii_uppercase() } else { ch };
                    let at = self.state.new_folder_cursor.min(self.state.new_name.len());
                    self.state.new_name.insert(at, ch);
                    self.state.new_folder_cursor = at + ch.len_utf8();
                } else {
                    self.typing().push(if shifted { ch.to_ascii_uppercase() } else { ch });
                }
                self.edits += 1;
                if shifted {
                    self.state.layer = Layer::Letters;
                }
            }
            Cap::Space => {
                if self.state.new_folder {
                    let at = self.state.new_folder_cursor.min(self.state.new_name.len());
                    self.state.new_name.insert(at, ' ');
                    self.state.new_folder_cursor = at + 1;
                } else {
                    self.typing().push(' ');
                }
                self.edits += 1;
            }
            Cap::Backspace => {
                if self.state.new_folder {
                    let at = self.state.new_folder_cursor.min(self.state.new_name.len());
                    if let Some((prev, _)) = self.state.new_name[..at].char_indices().next_back() {
                        self.state.new_name.replace_range(prev..at, "");
                        self.state.new_folder_cursor = prev;
                    }
                } else {
                    self.typing().pop();
                }
                self.edits += 1;
            }
            Cap::Shift => {
                self.state.layer = if self.state.layer == Layer::Shifted { Layer::Letters } else { Layer::Shifted };
            }
            Cap::Show(layer) => self.state.layer = layer,
            Cap::Hide => self.slide_keys(true, now),
            Cap::Enter => match self.screen() {
                Screen::Search => {
                    if let Some(first) = search::find_on(&self.state.search, l, |screen| self.state.offers(screen)).first() {
                        return self.act(Action::Open(first.screen), now);
                    }
                }
                Screen::Files if self.state.new_folder => return self.make_folder(now),
                Screen::Files => {
                    self.state.files_search = false;
                    self.state.files_query.clear();
                    self.state.files.set_query("");
                    self.refresh();
                }
                Screen::FileSearch => {}
                Screen::Decrypt if self.state.code.is_empty() => self.say(l.pick(texts::toast::ENTER_LOCK), now),
                Screen::Decrypt => {
                    let code = std::mem::take(&mut self.state.code);
                    return self.unlock(code);
                }
                // A name the folder cannot take is turned down here, in
                // the user's words; an unchanged one leaves the page.
                Screen::Rename => {
                    let name = self.state.new_name.clone();
                    let current = &self.state.file.as_ref().expect("the file page shows an entry").name;
                    let taken = self.state.files.entries.as_ref().is_ok_and(|entries| entries.iter().any(|e| e.name == name));
                    match name.as_str() {
                        "" => self.say(l.pick(texts::toast::ENTER_NAME), now),
                        "." | ".." => self.say(l.pick(texts::toast::NAME_DOTS), now),
                        _ if name.contains('/') => self.say(l.pick(texts::toast::NAME_SLASH), now),
                        _ if name == *current => return self.back(now),
                        _ if taken => self.say_taken(now),
                        _ => return vec![Effect::Rename { path: self.entry_path(), name }],
                    }
                }
                // A password the network's protection cannot take is turned
                // down here; one it can goes, and the WLAN page shows how
                // joining goes.
                Screen::WlanPassword => {
                    let network = self.state.joining.clone().expect("the password page is for a network");
                    match refused(l, network.security, &self.state.password) {
                        Some(why) => self.say(why, now),
                        None => {
                            let password = Secret(std::mem::take(&mut self.state.password));
                            let mut fx = self.back(now);
                            fx.push(Effect::JoinWlan { ssid: network.ssid, name: network.name, security: network.security, password });
                            return fx;
                        }
                    }
                }
                // So is a name no network can have; the keyboard then types
                // into the field turned down.
                Screen::WlanAdd => {
                    let a = self.state.adding.as_mut().expect("the page adds a network");
                    let (field, why) = match a.name.len() {
                        0 => (Field::Name, Some(l.pick(texts::toast::ENTER_NETWORK_NAME).to_string())),
                        n if n > SSID_MAX => (Field::Name, Some(texts::toast::network_name_long(l, SSID_MAX))),
                        _ if a.security.password() => (Field::Password, refused(l, a.security, &self.state.password).map(str::to_string)),
                        _ => (Field::Name, None),
                    };
                    match why {
                        Some(why) => {
                            if a.field != field {
                                a.field = field;
                                self.refresh();
                            }
                            self.say(&why, now)
                        }
                        None => {
                            let (name, security) = (a.name.clone(), a.security);
                            let password = match security.password() {
                                true => Secret(std::mem::take(&mut self.state.password)),
                                false => Secret::default(),
                            };
                            let mut fx = self.back(now);
                            fx.push(Effect::JoinWlan { ssid: name.clone().into_bytes(), name, security, password });
                            return fx;
                        }
                    }
                }
                other => unreachable!("{other:?} shows no keyboard"),
            },
        }
        // What is typed shows above the keyboard, which stays as it was
        // while its keys do.
        if matches!(self.screen(), Screen::Files | Screen::FileSearch) {
            self.state.files.set_query(&self.state.files_query);
            self.refresh();
        } else if matches!(cap, Cap::Char(_) | Cap::Space | Cap::Backspace) && self.state.layer == layer {
            let t = self.theme();
            self.refresh_rows(0, (t.m.h as f32 - widgets::keyboard_height(&t)).floor() as u32);
        } else {
            self.refresh();
        }
        Vec::new()
    }

    /// What a swipe-to-confirm control slid to its end does.
    pub(super) fn confirmed(&mut self, which: Confirm, now: u64) -> Vec<Effect> {
        let (l, mut fx) = (self.state.lang, Vec::new());
        match which {
            Confirm::Reboot => fx.push(Effect::Reboot(TARGETS[self.state.reboot])),
            Confirm::Wipe => {
                let chosen: Vec<Wipe> = WIPES.iter().zip(self.state.wipe).filter(|(_, on)| *on).map(|(&w, _)| w).collect();
                match chosen.is_empty() {
                    true => self.say(l.pick(texts::toast::CHOOSE_WIPE), now),
                    false => fx.push(Effect::Wipe(chosen)),
                }
            }
            Confirm::Install | Confirm::InstallWipe => {
                if let Install::Confirming { version, .. } = &self.state.install {
                    self.state.install = Install::Updating { version: version.clone(), stage: None, overwriting: false };
                    fx.push(Effect::Install);
                    self.refresh();
                }
            }
            Confirm::RunScript => {
                if self.state.install == Install::ConfirmingScript {
                    self.state.install = Install::Scripting { stage: None, running: false };
                    fx.push(Effect::Install);
                    self.refresh();
                }
            }
            Confirm::Backup => {
                let s = &self.state;
                let refused = if !s.backup.contains(&true) {
                    Some(texts::toast::CHOOSE_BACKUP)
                } else if (s.backup[DATA_PART] || s.backup[MEDIA_PART]) && s.data != Data::Open {
                    Some(texts::toast::DECRYPT_TO_BACK_UP)
                } else if s.place_open(s.backup_to) {
                    None
                } else {
                    Some(match (s.backup_to, &s.usb_drive) {
                        (Place::Storage, _) if s.data == Data::Erased => DATA_ERASED,
                        (Place::Storage, _) => DECRYPT_FIRST,
                        (Place::UsbDrive, Some(UsbDrive::Unreadable(_))) => USB_DRIVE_UNREADABLE,
                        (Place::UsbDrive, _) => USB_DRIVE_WANTED,
                    })
                };
                let backup = Effect::Backup { parts: s.backup, place: s.backup_to, compress: s.compress, verify: s.verify };
                match refused {
                    Some(why) => self.say(l.pick(why), now),
                    None => fx.push(backup),
                }
            }
            Confirm::Restore => {
                let (entry, parts) = self.state.restore.as_ref().expect("the restore page shows a backup");
                let restore = Effect::Restore { place: entry.place, name: entry.name.clone(), parts: *parts };
                // The files of the data partition and of internal storage go
                // back into a data partition that is open: one erased is
                // made again by the system, as it next starts.
                let files = parts[DATA_PART] || parts[MEDIA_PART];
                if !parts.contains(&true) {
                    self.say(l.pick(texts::toast::CHOOSE_RESTORE), now);
                } else if files && self.state.data == Data::Erased {
                    self.say(l.pick(texts::toast::RESTORE_ONCE_MADE), now);
                } else if files && self.state.data != Data::Open {
                    self.say(l.pick(texts::toast::DECRYPT_TO_RESTORE), now);
                } else {
                    fx.push(restore);
                }
            }
            Confirm::DeleteBackup => {
                let (entry, _) = self.state.restore.as_ref().expect("the restore page shows a backup");
                fx.push(Effect::DeleteBackup { place: entry.place, name: entry.name.clone() });
            }
            Confirm::Slot => match (&self.state.boot_slots, self.state.next_slot) {
                (Some(Ok(slots)), Some(pick)) if pick != slots.active => fx.push(Effect::SetSlot(pick)),
                (Some(Ok(slots)), _) => self.say(&texts::toast::slot_already_next(l, slots.active.letter()), now),
                _ => self.say(l.pick(texts::slot::UNREADABLE), now),
            },
            Confirm::FactoryReset => fx.push(Effect::FactoryReset),
            Confirm::Avb => {
                let unlocked = self.state.avb.as_ref().is_some_and(|s| s.as_ref().is_ok_and(|s| s.unlocked));
                if !self.state.avb_writable && (!self.state.avb_verity || !self.state.avb_verification) {
                    self.state.avb_notice = Some(l.pick(texts::local::AVB_UNSUPPORTED).to_string());
                    self.refresh();
                } else if !self.state.avb_verification_writable && !self.state.avb_verification {
                    self.state.avb_notice = Some(l.pick(texts::local::AVB_VERITY_ONLY).to_string());
                    self.refresh();
                } else if unlocked {
                    fx.push(Effect::SetAvb { verity: self.state.avb_verity, verification: self.state.avb_verification });
                } else {
                    let why = l.pick(texts::local::AVB_LOCKED_REFUSAL);
                    self.state.avb_notice = Some(why.to_string());
                    self.refresh();
                }
            }
            Confirm::KernelSu => {
                let unlocked = self
                    .state
                    .kernel_su
                    .as_ref()
                    .is_some_and(|s| s.as_ref().is_ok_and(|s| s.unlocked));
                let other = self.state.magisk.as_ref().is_some_and(|s| s.as_ref().is_ok_and(|s| s.installed));
                let other_busy = self.state.magisk_job.as_ref().is_some_and(|j| j.outcome.is_none());
                if other {
                    self.say(l.pick(texts::root::OTHER_ROOT), now);
                } else if other_busy {
                    self.say(l.pick(texts::magisk::BUSY), now);
                } else if !unlocked {
                    self.say(l.pick(texts::kernel_su::LOCKED), now);
                } else if self.state.data != Data::Open {
                    self.say(l.pick(texts::kernel_su::NO_DATA), now);
                } else if self.state.kernel_su_job.as_ref().is_some_and(|j| j.outcome.is_none()) {
                    self.say(l.pick(texts::kernel_su::BUSY), now);
                } else if let Some(kmi) = self.kernel_su_kmi().map(|k| k.id.clone()) {
                    self.kernel_su_started();
                    let module = self.state.kernel_su_ko.clone();
                    let effect = if self.state.kernel_su_online {
                        Effect::InstallKernelSuLatest { kmi, module }
                    } else {
                        Effect::InstallKernelSu { kmi, module }
                    };
                    fx.push(effect);
                }
            }
            Confirm::Magisk => {
                let unlocked = self.state.magisk.as_ref().is_some_and(|s| s.as_ref().is_ok_and(|s| s.unlocked));
                let other = self.state.kernel_su.as_ref().is_some_and(|s| s.as_ref().is_ok_and(|s| s.installed));
                let other_busy = self.state.kernel_su_job.as_ref().is_some_and(|j| j.outcome.is_none());
                if other {
                    self.say(l.pick(texts::root::OTHER_ROOT), now);
                } else if other_busy {
                    self.say(l.pick(texts::kernel_su::BUSY), now);
                } else if !unlocked {
                    self.say(l.pick(texts::kernel_su::LOCKED), now);
                } else if self.state.data != Data::Open {
                    self.say(l.pick(texts::kernel_su::NO_DATA), now);
                } else if self.state.magisk_job.as_ref().is_some_and(|j| j.outcome.is_none()) {
                    self.say(l.pick(texts::magisk::BUSY), now);
                } else {
                    self.magisk_started();
                    fx.push(Effect::InstallMagisk);
                }
            }
            Confirm::Merge => {
                if self.state.update == Standing::Waiting {
                    self.state.update = Standing::Merging(0.0);
                    self.refresh();
                    fx.push(Effect::Merge);
                }
            }
            Confirm::DeleteEntry => fx.push(Effect::Delete(self.entry_path())),
            // The partition has to be open; whether its users have unlocked
            // their own files makes no difference.
            Confirm::Relabel => match self.state.data {
                Data::Open | Data::Locked { .. } => fx.push(Effect::Relabel),
                Data::Opening => self.say(l.pick(DATA_OPENING), now),
                Data::Erased => self.say(l.pick(DATA_ERASED), now),
                Data::Closed | Data::Failed(_) => self.say(l.pick(texts::toast::OPEN_DATA_FIRST), now),
            },
            // The data partition is reached through its own key whether or
            // not it is open, but not while it is being opened or erased.
            Confirm::Filesystem => {
                let row = self.fs_row().expect("the page of a partition shows one that was read");
                let data = row.name == DATA_PARTITION;
                match (&self.state.fs_action, &self.state.data) {
                    (_, Data::Opening) if data => self.say(l.pick(DATA_OPENING), now),
                    (FsAction::Repair, Data::Erased) if data => self.say(l.pick(DATA_ERASED), now),
                    (action, _) => fx.push(Effect::Filesystem { name: row.name.clone(), action: action.clone() }),
                }
            }
            Confirm::Flash => {
                if let Install::Choosing(choice) = &self.state.install {
                    let FlashTarget { name, place } = choice.partitions[choice.partition].clone();
                    fx.push(Effect::Flash { partition: name.clone(), place, pick: choice.pick });
                    self.state.install = Install::Flashing { partition: name, stage: None };
                    self.refresh();
                }
            }
            Confirm::SwitchSystem => {
                if let (Some(Ok(systems)), Some(pick)) = (&self.state.systems, self.state.system_pick)
                    && pick != starting(systems)
                {
                    fx.push(Effect::SwitchSystem(pick));
                    self.state.systems_work = Some(SystemsWork::Switching(pick));
                    self.state.system_pick = None;
                    self.refresh();
                }
            }
            Confirm::RemoveSystem => {
                fx.push(Effect::RemoveSystem);
                self.state.systems_work = Some(SystemsWork::Removing);
                self.state.asking_delete = false;
                self.state.system_pick = None;
                self.refresh();
            }
            Confirm::InstallSystem => {
                if let Install::SystemChoosing(choice) = &self.state.install {
                    fx.push(Effect::InstallSystem { size: choice.sizes[choice.size] });
                    self.state.install = Install::Systeming { name: choice.name.clone(), stage: None };
                    self.refresh();
                }
            }
        }
        fx
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::testing::*;

    #[test]
    fn search_finds_by_pinyin_typed_on_its_own_keyboard() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Search, &f, &i);
        type_text(&mut app, &f, &i, "bf");
        assert_eq!(app.state.search, "bf");
        let opens: Vec<Action> = app.frame(&f, &i).hits.iter().map(|h| h.action).filter(|a| matches!(a, Action::Open(_))).collect();
        assert_eq!(opens, [Action::Open(Screen::Backup)]);
        tap(&mut app, &f, &i, Action::Type(Cap::Backspace));
        assert_eq!(app.state.search, "b");
        tap(&mut app, &f, &i, Action::ClearSearch);
        assert_eq!(app.state.search, "");
        type_text(&mut app, &f, &i, "xpsj");
        tap(&mut app, &f, &i, Action::Type(Cap::Enter));
        assert_eq!(app.screen(), Screen::ScreenOff, "return opens the first result");
        app.key(Key::Back, 0);
        assert_eq!(app.screen(), Screen::Search);
    }

    #[test]
    fn shift_makes_one_capital_and_the_layers_switch() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Search, &f, &i);
        tap(&mut app, &f, &i, Action::Type(Cap::Shift));
        type_text(&mut app, &f, &i, "ab");
        assert_eq!(app.state.search, "Ab");
        tap(&mut app, &f, &i, Action::Type(Cap::Show(Layer::Digits)));
        type_text(&mut app, &f, &i, "1");
        tap(&mut app, &f, &i, Action::Type(Cap::Show(Layer::Symbols)));
        type_text(&mut app, &f, &i, "|");
        tap(&mut app, &f, &i, Action::Type(Cap::Space));
        assert_eq!(app.state.search, "Ab1| ");
        tap(&mut app, &f, &i, Action::Type(Cap::Show(Layer::Letters)));
        assert_eq!(app.state.layer, Layer::Letters);
    }

    #[test]
    fn a_phone_whose_bootloader_checks_vbmeta_can_only_have_its_checks_turned_on() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Avb, W, H, PPD);
        app.state.avb_writable = false;
        app.set_avb(Ok(AvbStatus { partition: "vbmeta_b".into(), verity: false, verification: true, unlocked: true }));
        assert!(app.act(Action::Toggle(Switch::AvbVerification), 0).is_empty(), "a check is not turned off here");
        assert!(app.state.avb_verification);
        assert!(app.act(Action::Toggle(Switch::AvbVerity), 0).is_empty());
        assert!(app.state.avb_verity, "a check already off is turned back on");
        assert_eq!(
            app.confirmed(Confirm::Avb, 0),
            vec![Effect::SetAvb { verity: true, verification: true }],
            "the original flag bytes are put back"
        );
        let mut c = crate::Canvas::new(W, H);
        app.compose(&mut c, &f, &i, "12:00", 0);
    }

    #[test]
    fn a_phone_whose_bootloader_checks_the_boot_image_may_still_turn_verity_off() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Avb, W, H, PPD);
        app.state.avb_verification_writable = false;
        app.set_avb(Ok(AvbStatus { partition: "vbmeta_b".into(), verity: true, verification: true, unlocked: true }));
        let frame = app.frame(&f, &i);
        assert!(frame.hits.iter().any(|hit| hit.action == Action::Toggle(Switch::AvbVerity)));
        assert!(!frame.hits.iter().any(|hit| hit.action == Action::Toggle(Switch::AvbVerification)), "boot verification is a status, not a control");
        assert!(app.act(Action::Toggle(Switch::AvbVerification), 0).is_empty(), "boot verification is not turned off here");
        assert!(app.state.avb_verification);
        assert!(app.act(Action::Toggle(Switch::AvbVerity), 0).is_empty());
        assert!(!app.state.avb_verity, "dm-verity is turned off");
        assert_eq!(
            app.confirmed(Confirm::Avb, 0),
            vec![Effect::SetAvb { verity: false, verification: true }],
            "the verity flag alone is written"
        );
        let mut c = crate::Canvas::new(W, H);
        app.compose(&mut c, &f, &i, "12:00", 0);
    }
}
