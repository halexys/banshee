// Banshee top-bar icon. Installed and kept up to date by the Banshee binary itself.
//
// GNOME's AppIndicator support opens a plain menu on click and never tells the app; this
// button instead asks Banshee (over its Unix socket) to toggle the quick panel, and moves
// that window right under the icon. It also shows CPU/GPU temperatures coloured by
// threshold, and notifies when either stays very hot.

import GLib from 'gi://GLib';
import Gio from 'gi://Gio';
import St from 'gi://St';
import Clutter from 'gi://Clutter';
import GObject from 'gi://GObject';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as MessageTray from 'resource:///org/gnome/shell/ui/messageTray.js';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

const PANEL_TITLE = 'Banshee Panel';
const POLL_SECONDS = 2;
const WARN = 70, HOT = 85;                 // temperature colours
const ALERT = 90, ALERT_SECONDS = 10;      // notification: this hot for this long
const C_OK = '#E6EDF3', C_WARN = '#FFC15D', C_HOT = '#FF5C8A';

function sockPath() {
    return GLib.build_filenamev([GLib.get_user_runtime_dir(), 'banshee.sock']);
}

// One request/response line over Banshee's single-instance socket.
function ask(cmd) {
    return new Promise((resolve, reject) => {
        const client = new Gio.SocketClient();
        client.connect_async(Gio.UnixSocketAddress.new(sockPath()), null, (c, res) => {
            let conn;
            try {
                conn = c.connect_finish(res);
                conn.get_output_stream().write_all(new TextEncoder().encode(`${cmd}\n`), null);
                const din = new Gio.DataInputStream({base_stream: conn.get_input_stream()});
                din.read_line_async(GLib.PRIORITY_DEFAULT, null, (d, r2) => {
                    try {
                        const [line] = d.read_line_finish_utf8(r2);
                        resolve(line ?? '');
                    } catch (e) {
                        reject(e);
                    } finally {
                        conn.close(null);
                    }
                });
            } catch (e) {
                conn?.close(null);
                reject(e);
            }
        });
    });
}

const tempColor = t => (t == null ? C_OK : t >= HOT ? C_HOT : t >= WARN ? C_WARN : C_OK);

const BansheeButton = GObject.registerClass(
class BansheeButton extends PanelMenu.Button {
    _init(ext) {
        super._init(0.5, 'Banshee', true);
        this._ext = ext;
        this._target = null;
        this._hotSince = 0;
        this._alerted = false;

        const box = new St.BoxLayout({style_class: 'panel-status-menu-box', y_align: Clutter.ActorAlign.CENTER});
        this._icon = new St.Icon({
            gicon: Gio.icon_new_for_string(GLib.build_filenamev([ext.path, 'banshee-symbolic.svg'])),
            style_class: 'system-status-icon',
        });
        this._cpu = new St.Label({y_align: Clutter.ActorAlign.CENTER, style: 'font-weight: bold; margin-left: 4px;'});
        this._gpu = new St.Label({y_align: Clutter.ActorAlign.CENTER, style: 'font-weight: bold; margin-left: 6px;'});
        box.add_child(this._icon);
        box.add_child(this._cpu);
        box.add_child(this._gpu);
        this.add_child(box);
        this.visible = false;

        this.connect('button-press-event', () => {
            this._toggle();
            return Clutter.EVENT_STOP;
        });
        this.connect('touch-event', (_a, ev) => {
            if (ev.type() === Clutter.EventType.TOUCH_BEGIN)
                this._toggle();
            return Clutter.EVENT_STOP;
        });

        // The panel window is (re)created on every show: place it the moment it appears.
        this._winId = global.display.connect('window-created', (_d, win) => this._place(win));
        this._poll();
        this._timer = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, POLL_SECONDS, () => {
            this._poll();
            return GLib.SOURCE_CONTINUE;
        });
    }

    _toggle() {
        const [x, y] = this.get_transformed_position();
        const [w, h] = this.get_transformed_size();
        this._target = {right: x + w, bottom: y + h, center: x + w / 2};
        ask('toggle').catch(() => {});
    }

    _place(win) {
        const move = () => {
            if (win.get_title() !== PANEL_TITLE || !this._target)
                return false;
            const r = win.get_frame_rect();
            const mon = Main.layoutManager.monitors[win.get_monitor()] ?? Main.layoutManager.primaryMonitor;
            let x = Math.round(this._target.center - r.width / 2);
            x = Math.max(mon.x + 8, Math.min(x, mon.x + mon.width - r.width - 8));
            win.move_frame(true, x, Math.round(this._target.bottom + 6));
            return true;
        };
        if (!move()) {
            const id = win.connect('notify::title', () => {
                if (move())
                    win.disconnect(id);
            });
            win.connect('unmanaged', () => {
                try {
                    win.disconnect(id);
                } catch (e) {}
            });
        }
    }

    async _poll() {
        let s;
        try {
            s = JSON.parse(await ask('state'));
        } catch (e) {
            this.visible = false;   // Banshee not running
            return;
        }
        this.visible = true;
        this._icon.set_style(`color: ${s.color};`);
        this._cpu.text = s.cpu != null ? `${s.cpu}°` : '';
        this._cpu.set_style(`font-weight: bold; margin-left: 4px; color: ${tempColor(s.cpu)};`);
        this._gpu.text = s.gpu != null ? `${s.gpu}°` : '';
        this._gpu.set_style(`font-weight: bold; margin-left: 6px; color: ${tempColor(s.gpu)};`);
        this.accessible_name = `Banshee · ${s.name}`;
        this._checkAlert(s);
    }

    _checkAlert(s) {
        const t = Math.max(s.cpu ?? 0, s.gpu ?? 0);
        const now = GLib.get_monotonic_time() / 1e6;
        if (t < ALERT) {
            this._hotSince = 0;
            if (t < ALERT - 5)
                this._alerted = false;   // re-arm once it has really cooled down
            return;
        }
        if (!this._hotSince)
            this._hotSince = now;
        if (this._alerted || now - this._hotSince < ALERT_SECONDS)
            return;
        this._alerted = true;
        const who = (s.cpu ?? 0) >= (s.gpu ?? 0) ? `CPU ${s.cpu} °C` : `GPU ${s.gpu} °C`;
        this._notify(`Temperatura alta: ${who}`, `Más de ${ALERT} °C durante ${ALERT_SECONDS} s · perfil ${s.name}`);
    }

    _notify(title, body) {
        try {
            const source = new MessageTray.Source({title: 'Banshee', iconName: 'dialog-warning-symbolic'});
            Main.messageTray.add(source);
            const n = new MessageTray.Notification({source, title, body, urgency: MessageTray.Urgency.HIGH});
            source.addNotification(n);
        } catch (e) {
            Main.notify(title, body);
        }
    }

    destroy() {
        if (this._timer)
            GLib.source_remove(this._timer);
        this._timer = 0;
        if (this._winId)
            global.display.disconnect(this._winId);
        this._winId = 0;
        super.destroy();
    }
});

export default class BansheeExtension extends Extension {
    enable() {
        this._button = new BansheeButton(this);
        Main.panel.addToStatusArea(this.uuid, this._button);
    }

    disable() {
        this._button?.destroy();
        this._button = null;
    }
}
