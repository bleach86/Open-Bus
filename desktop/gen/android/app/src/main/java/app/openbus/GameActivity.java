package app.openbus;

import android.os.Bundle;
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
