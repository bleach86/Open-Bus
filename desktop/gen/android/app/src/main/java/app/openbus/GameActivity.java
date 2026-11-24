package app.openbus;

import android.content.pm.PackageManager;
import android.graphics.drawable.GradientDrawable;
import android.hardware.input.InputManager;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.view.ContextThemeWrapper;
import android.view.Gravity;
import android.view.InputDevice;
import android.view.KeyEvent;
import android.util.TypedValue;
import android.view.View;
import android.view.ViewGroup;
import android.view.WindowManager;
import android.widget.Button;
import android.widget.FrameLayout;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
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

    /**
     * A touch screen's pause: what it offers, as the game's own menu does;
     * and, with the touch screen's gamepad, its moving.
     */
    private static final String RESUME = "Resume", SAVE = "Save state", LOAD = "Load state";
    private static final String RESET = "Reset", MOVE = "Move controls", QUIT = "Quit";
    /** How long Reset's held, F12, for the emulator to see it held. */
    private static final long RESET_HELD_MS = 250;

    /** A touch screen's gamepad, for the NES, or null: the app's not on one. */
    private TouchPad touchPad;
    /**
     * What's over the game, the pause or the controls being moved, or null:
     * views in the game's own window, not dialogs, which are windows of
     * their own and bring Android's bars up over the game, moving it.
     */
    private View over;

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
        // The whole screen the game's, beside a camera's cutout too, its
        // bars shown or not: as SDL means it to be, but sets without
        // applying it. Otherwise, as a dialog brings Android's bars up, the
        // screen's narrowed by the cutout, and the touch screen's gamepad
        // with it, over the picture.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            WindowManager.LayoutParams attributes = getWindow().getAttributes();
            attributes.layoutInDisplayCutoutMode =
                    WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
            getWindow().setAttributes(attributes);
        }
        String[] args = getIntent().getStringArrayExtra("args");
        if (args == null) {
            finish();
            return;
        }
        if (args.length > 0 && args[0].equals("nes") && !mBrokenLibraries && mLayout != null
                && getPackageManager().hasSystemFeature(PackageManager.FEATURE_TOUCHSCREEN)) {
            touchPad = new TouchPad(this, GameActivity::touched, false);
            mLayout.addView(touchPad, new ViewGroup.LayoutParams(
                    ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
            showTouchPad();
            getSystemService(InputManager.class).registerInputDeviceListener(pads, null);
        }
    }

    /** The touch screen's gamepad gone as a gamepad's plugged in, back as it's taken out. */
    private final InputManager.InputDeviceListener pads = new InputManager.InputDeviceListener() {
        @Override
        public void onInputDeviceAdded(int id) {
            showTouchPad();
        }

        @Override
        public void onInputDeviceRemoved(int id) {
            showTouchPad();
        }

        @Override
        public void onInputDeviceChanged(int id) {
            showTouchPad();
        }
    };

    private void showTouchPad() {
        boolean shown = !padPlugged();
        if (!shown) {
            touchPad.letGo();
        }
        touchPad.setVisibility(shown ? ViewGroup.VISIBLE : ViewGroup.GONE);
    }

    /** The touch screen's buttons held, as the emulator's touch module has them. */
    private static native void touched(int buttons);

    /**
     * The game held still as anything takes its focus, Android's
     * notifications pulled down: SDL only pauses it as the app's put away,
     * where a phone could show two apps at once (any since Android 7); and
     * going again with its focus back, unless something's over it. Showing
     * beside another app, it's left going, as SDL leaves it. What's held on
     * the screen's let go.
     */
    @Override
    public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        if (mBrokenLibraries) {
            return;
        }
        if (hasFocus) {
            if (over == null) {
                still(false);
            }
            return;
        }
        if (touchPad != null) {
            touchPad.letGo();
        }
        if (!isInMultiWindowMode()) {
            still(true);
        }
    }

    /**
     * The game held still, or let go again, by the emulator: not by SDL's
     * pause, which takes its window out of fullscreen, bringing Android's
     * bars up over the game, and moving it for them.
     */
    private static native void still(boolean still);

    /** `view` over the whole game, the game paused under it. */
    private void layOver(View view) {
        over = view;
        mLayout.addView(view, new ViewGroup.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
        if (touchPad != null) {
            touchPad.letGo();
        }
        still(true);
    }

    /**
     * What's over the game gone, and the game `going` again; the touch
     * screen's gamepad as it's been made, if it was being moved.
     */
    private void closeOver(boolean going) {
        if (over == null) {
            return;
        }
        mLayout.removeView(over);
        over = null;
        if (touchPad != null) {
            touchPad.reload();
            showTouchPad();
        }
        if (going) {
            still(false);
        }
    }

    /**
     * Its process ended with it: each game a fresh one, as each is its own
     * program on a desktop, nothing of the last left in the emulator.
     */
    @Override
    protected void onDestroy() {
        if (touchPad != null) {
            getSystemService(InputManager.class).unregisterInputDeviceListener(pads);
        }
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
     * Back, on a touch screen with no gamepad: the pause, its choices big
     * enough for a finger, not the game's menu, which only a gamepad or
     * keys can work. With a gamepad, and on a TV (its remote has no touch
     * screen), Back's the game's menu, as it was. With something over the
     * game, Back closes it, the game going again.
     */
    @Override
    public boolean dispatchKeyEvent(KeyEvent event) {
        if (event.getKeyCode() == KeyEvent.KEYCODE_BACK && over != null) {
            if (event.getAction() == KeyEvent.ACTION_UP && !event.isCanceled()) {
                closeOver(true);
            }
            return true;
        }
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
        return !isPad(event.getSource()) && !padPlugged();
    }

    /** Whether there's a gamepad plugged in or paired. */
    private static boolean padPlugged() {
        for (int id : InputDevice.getDeviceIds()) {
            InputDevice device = InputDevice.getDevice(id);
            if (device != null && plugged(device) && isPad(device.getSources())) {
                return true;
            }
        }
        return false;
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
     * The pause, over the game in its own window, the game standing still
     * under it: what's chosen done as it's gone, the game going again.
     * Tapped off it, or Back, it's resumed.
     */
    private void pause() {
        if (over != null || mLayout == null) {
            return;
        }
        List<String> offered = new ArrayList<>(Arrays.asList(RESUME, SAVE, LOAD, RESET));
        if (touchPad != null && touchPad.getVisibility() == View.VISIBLE) {
            offered.add(MOVE);
        }
        offered.add(QUIT);

        FrameLayout screen = new FrameLayout(this);
        screen.setBackgroundColor(0x99000000);
        screen.setOnClickListener(v -> closeOver(true));
        LinearLayout card = new LinearLayout(this);
        card.setOrientation(LinearLayout.VERTICAL);
        card.setClickable(true);
        GradientDrawable back = new GradientDrawable();
        back.setColor(0xff202124);
        back.setCornerRadius(dp(24));
        card.setBackground(back);
        card.setPadding(0, dp(12), 0, dp(12));
        TextView title = words("Paused", 22);
        title.setPadding(dp(24), dp(4), dp(24), dp(8));
        card.addView(title);
        TypedValue ripple = new TypedValue();
        getTheme().resolveAttribute(android.R.attr.selectableItemBackground, ripple, true);
        for (String item : offered) {
            TextView row = words(item, 18);
            row.setMinHeight(dp(48));
            row.setGravity(Gravity.CENTER_VERTICAL);
            row.setPadding(dp(24), 0, dp(24), 0);
            row.setBackgroundResource(ripple.resourceId);
            row.setOnClickListener(v -> {
                // Moving the controls, it stays paused for them.
                closeOver(!MOVE.equals(item));
                act(item);
            });
            card.addView(row);
        }
        ScrollView scroll = new ScrollView(this);
        scroll.addView(card);
        FrameLayout.LayoutParams middle = new FrameLayout.LayoutParams(
                dp(360), ViewGroup.LayoutParams.WRAP_CONTENT, Gravity.CENTER);
        middle.setMargins(0, dp(8), 0, dp(8));
        screen.addView(scroll, middle);
        layOver(screen);
    }

    private TextView words(String text, float sp) {
        TextView words = new TextView(this);
        words.setText(text);
        words.setTextColor(0xffe8eaed);
        words.setTextSize(TypedValue.COMPLEX_UNIT_SP, sp);
        return words;
    }

    private int dp(float dp) {
        return (int) (dp * getResources().getDisplayMetrics().density + 0.5f);
    }

    /** What's chosen done: as the game's menu does it, by its keys. */
    private void act(String choice) {
        if (SAVE.equals(choice)) {
            press(KeyEvent.KEYCODE_F2);
        } else if (LOAD.equals(choice)) {
            press(KeyEvent.KEYCODE_F4);
        } else if (RESET.equals(choice)) {
            SDLActivity.onNativeKeyDown(KeyEvent.KEYCODE_F12);
            new Handler(Looper.getMainLooper()).postDelayed(
                    () -> SDLActivity.onNativeKeyUp(KeyEvent.KEYCODE_F12), RESET_HELD_MS);
        } else if (MOVE.equals(choice)) {
            moveControls();
        } else if (QUIT.equals(choice)) {
            // As Android closing it: SDL's quit, the game saving as it
            // goes, and back to the library.
            finish();
        }
    }

    /**
     * The touch screen's gamepad moved and resized: a copy of it over the
     * whole game, which stands still under it, each piece dragged where
     * it's wanted, kept as it's let go; Smaller, Bigger and Reset at the
     * top, under them each press's tick turned on or off and which of A and
     * B is higher; and Done, or Back, the game going again with it as it's
     * been made.
     */
    private void moveControls() {
        TouchPad editing = new TouchPad(this, buttons -> {}, true);
        FrameLayout screen = new FrameLayout(this);
        screen.setBackgroundColor(0x66000000);
        screen.addView(editing, new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
        LinearLayout tools = new LinearLayout(this);
        tools.setOrientation(LinearLayout.VERTICAL);
        LinearLayout sizes = new LinearLayout(this), choices = new LinearLayout(this);
        choices.setGravity(Gravity.CENTER_HORIZONTAL);
        Button ticks = tool("", null);
        Button higher = tool("", null);
        Runnable named = () -> {
            ticks.setText(editing.ticks() ? "Vibrate: on" : "Vibrate: off");
            higher.setText(editing.bHigher() ? "B higher" : "A higher");
        };
        ticks.setOnClickListener(v -> {
            editing.ticks(!editing.ticks());
            named.run();
        });
        higher.setOnClickListener(v -> {
            editing.bHigher(!editing.bHigher());
            named.run();
        });
        named.run();
        sizes.addView(tool("Smaller", v -> editing.resize(-0.1f)));
        sizes.addView(tool("Bigger", v -> editing.resize(0.1f)));
        sizes.addView(tool("Reset", v -> {
            editing.reset();
            named.run();
        }));
        sizes.addView(tool("Done", v -> closeOver(true)));
        choices.addView(ticks);
        choices.addView(higher);
        tools.addView(sizes);
        tools.addView(choices);
        FrameLayout.LayoutParams top = new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT,
                Gravity.TOP | Gravity.CENTER_HORIZONTAL);
        top.topMargin = dp(12);
        screen.addView(tools, top);
        // The game's own hidden as its copy's moved, back as it's done.
        touchPad.setVisibility(View.INVISIBLE);
        layOver(screen);
    }

    private Button tool(String name, View.OnClickListener clicked) {
        Button button = new Button(new ContextThemeWrapper(this, android.R.style.Theme_DeviceDefault));
        button.setText(name);
        button.setOnClickListener(clicked);
        return button;
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
