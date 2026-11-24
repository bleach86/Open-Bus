package app.openbus;

import android.app.AlertDialog;
import android.content.pm.PackageManager;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.view.InputDevice;
import android.view.KeyEvent;
import java.io.FileOutputStream;
import java.io.IOException;
import android.system.ErrnoException;
import android.system.Os;
import org.libsdl.app.SDLActivity;

/**
 * The game screen: SDL's activity, running the emulator's library,
 * libcpu_emu.so, as a program, with the command line the library starts it
 * with: the string array extra "args", as the desktop app gives the
 * emulator its arguments ("nes", the game's path, and its options); run in
 * the folder "folder", a panic told in the file "log". The library told by
 * that file too that it's started, as it's made, and that it's over, as it
 * says so. Its settings in the app's private folder. Started with no game, there's nothing to run: it
 * closes. It runs in a process of its own, ":game", ended as it closes.
 */
public class GameActivity extends SDLActivity {
    /** What the library's told, as a game closes. */
    private static final String ENDED = "ended\n";

    /** A touch screen's pause: what it offers, as the game's own menu does. */
    private static final String[] PAUSED = {"Resume", "Save state", "Load state", "Reset", "Quit"};
    private static final int SAVE = 1, LOAD = 2, RESET = 3, QUIT = 4;
    /** How long Reset's held, F12, for the emulator to see it held. */
    private static final long RESET_HELD_MS = 250;

    private AlertDialog paused;
    /** What was chosen from it, or -1, to be done as it's gone. */
    private int chosen = -1;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        getExternalFilesDir(null);
        // The emulator's settings, its controls, in the app's own folder:
        // where it looks for them on Linux, as Android has no home.
        try {
            Os.setenv("XDG_CONFIG_HOME", getFilesDir().getPath(), true);
        } catch (ErrnoException e) {
            // Its controls as they come, then: not kept.
        }
        for (String[] told : new String[][] {{"folder", "OPENBUS_FOLDER"}, {"log", "OPENBUS_LOG"}}) {
            String value = getIntent().getStringExtra(told[0]);
            try {
                if (value != null) {
                    Os.setenv(told[1], value, true);
                }
            } catch (ErrnoException e) {
                // Run where it's started, then; a panic only in Android's log.
            }
        }
        tell("", false);
        super.onCreate(savedInstanceState);
        if (getIntent().getStringArrayExtra("args") == null) {
            finish();
        }
    }

    /**
     * Its process ended with it: each game a fresh one, as each is its own
     * program on a desktop, nothing of the last left in the emulator.
     */
    @Override
    protected void onDestroy() {
        super.onDestroy();
        tell(ENDED, true);
        System.exit(0);
    }

    /** `said` written to the log, after what's there or in its place. */
    private void tell(String said, boolean after) {
        String log = getIntent().getStringExtra("log");
        if (log == null) {
            return;
        }
        try (FileOutputStream out = new FileOutputStream(log, after)) {
            out.write(said.getBytes());
        } catch (IOException e) {
            // Not told, then: the library goes by its process.
        }
    }

    /**
     * Back, on a touch screen with no gamepad: Android's own dialog, its
     * choices big enough for a finger, not the game's menu, which only a
     * gamepad or keys can work. With a gamepad, and on a TV (its remote has
     * no touch screen), Back's the game's menu, as it was.
     */
    @Override
    public boolean dispatchKeyEvent(KeyEvent event) {
        if (event.getKeyCode() == KeyEvent.KEYCODE_BACK && byTouch(event)) {
            if (event.getAction() == KeyEvent.ACTION_UP && !event.isCanceled()) {
                pause();
            }
            return true;
        }
        return super.dispatchKeyEvent(event);
    }

    /** Whether Back's a touch screen's: there's one, and no gamepad. */
    private boolean byTouch(KeyEvent event) {
        if (!getPackageManager().hasSystemFeature(PackageManager.FEATURE_TOUCHSCREEN)) {
            return false;
        }
        if (isPad(event.getSource())) {
            return false;
        }
        for (int id : InputDevice.getDeviceIds()) {
            InputDevice device = InputDevice.getDevice(id);
            if (device != null && plugged(device) && isPad(device.getSources())) {
                return false;
            }
        }
        return true;
    }

    /**
     * Whether `device` is one plugged in or paired, not the phone's own: a
     * Pixel's fingerprint reader says it's a joystick, its buttons a
     * gamepad. Before Android 10, which can't tell, any that isn't virtual.
     */
    private static boolean plugged(InputDevice device) {
        if (device.isVirtual()) {
            return false;
        }
        return Build.VERSION.SDK_INT < Build.VERSION_CODES.Q || device.isExternal();
    }

    /**
     * Whether `sources` are a gamepad's or a joystick's: all of either's
     * bits, as a keyboard's share a gamepad's class of button.
     */
    private static boolean isPad(int sources) {
        int pad = InputDevice.SOURCE_GAMEPAD, stick = InputDevice.SOURCE_JOYSTICK;
        return (sources & pad) == pad || (sources & stick) == stick;
    }

    /**
     * The pause: the game stands still behind it (SDL's activity pauses it
     * as the dialog takes its focus), and what's chosen is done as it's
     * gone, the game going again. Tapped off it, or Back, it's resumed.
     */
    private void pause() {
        if (paused != null && paused.isShowing()) {
            return;
        }
        chosen = -1;
        paused = new AlertDialog.Builder(this, AlertDialog.THEME_DEVICE_DEFAULT_DARK)
                .setTitle("Paused")
                .setItems(PAUSED, (dialog, which) -> chosen = which)
                .setOnDismissListener(dialog -> act(chosen))
                .create();
        paused.show();
    }

    /** What's chosen done: as the game's menu does it, by its keys. */
    private void act(int choice) {
        switch (choice) {
            case SAVE:
                press(KeyEvent.KEYCODE_F2);
                break;
            case LOAD:
                press(KeyEvent.KEYCODE_F4);
                break;
            case RESET:
                SDLActivity.onNativeKeyDown(KeyEvent.KEYCODE_F12);
                new Handler(Looper.getMainLooper()).postDelayed(
                        () -> SDLActivity.onNativeKeyUp(KeyEvent.KEYCODE_F12), RESET_HELD_MS);
                break;
            case QUIT:
                // As Android closing it: SDL's quit, the game saving as it
                // goes, and back to the library.
                finish();
                break;
            default:
                break;
        }
    }

    private static void press(int keyCode) {
        SDLActivity.onNativeKeyDown(keyCode);
        SDLActivity.onNativeKeyUp(keyCode);
    }

    @Override
    protected String[] getLibraries() {
        return new String[] {"SDL2", "cpu_emu"};
    }

    @Override
    protected String[] getArguments() {
        String[] args = getIntent().getStringArrayExtra("args");
        return args != null ? args : new String[0];
    }
}
