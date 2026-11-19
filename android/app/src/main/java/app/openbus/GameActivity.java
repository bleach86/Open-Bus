package app.openbus;

import android.os.Bundle;
import android.system.ErrnoException;
import android.system.Os;
import org.libsdl.app.SDLActivity;

/**
 * The game screen: SDL's activity, running the emulator's library,
 * libcpu_emu.so, as a program, with the command line it's started with:
 * the string array extra "args", as the desktop app gives the emulator its
 * arguments ("nes", the game's path, and its options). The app's own
 * folder, where its games are kept, made if it isn't there yet; its
 * settings in its private one. Started with no game, there's nothing to
 * run: it closes. It runs in a process of its own, ":game", ended as it
 * closes.
 */
public class GameActivity extends SDLActivity {
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
        System.exit(0);
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
