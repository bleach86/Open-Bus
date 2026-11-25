package app.openbus;

import android.content.Context;
import android.graphics.drawable.GradientDrawable;
import android.os.Handler;
import android.os.Looper;
import android.os.SystemClock;
import android.text.InputType;
import android.util.TypedValue;
import android.view.Gravity;
import android.view.HapticFeedbackConstants;
import android.view.KeyCharacterMap;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.View;
import android.view.inputmethod.BaseInputConnection;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;
import android.view.inputmethod.InputMethodManager;
import android.widget.LinearLayout;
import android.widget.TextView;
import org.libsdl.app.SDLActivity;

/**
 * A computer's keyboard on a touch screen, as Termux has one: Android's own
 * keyboard, and over it two rows of the keys it hasn't (Esc, Tab, Ctrl,
 * Alt, the arrows and the rest). What's typed on either goes to the game
 * as a keyboard's keys go, pressed and let go, Shift and all, and as the
 * text they type: a machine that takes its keys as they're pressed (the
 * Commander X16) and one that takes what they type (the Apple II) both
 * have it. Android's keyboard's asked for as a password's is, so it never
 * corrects, suggests or capitalises: every key's sent as it's pressed.
 * Ctrl and Alt are sticky: tapped, the next key's pressed with them;
 * tapped twice, they're held till they're tapped again, lit while they
 * are. A key held down repeats, as a keyboard's does.
 */
public class Keyboard extends LinearLayout {
    /** A key of the rows: its label and Android's key code, or one of these. */
    private static final int CTRL = -1, ALT = -2, TYPING = -3;
    private static final Object[][][] ROWS = {
        {{"ESC", KeyEvent.KEYCODE_ESCAPE}, {"TAB", KeyEvent.KEYCODE_TAB},
            {"HOME", KeyEvent.KEYCODE_MOVE_HOME}, {"↑", KeyEvent.KEYCODE_DPAD_UP},
            {"END", KeyEvent.KEYCODE_MOVE_END}, {"PGUP", KeyEvent.KEYCODE_PAGE_UP},
            {"DEL", KeyEvent.KEYCODE_FORWARD_DEL}},
        {{"CTRL", CTRL}, {"ALT", ALT}, {"←", KeyEvent.KEYCODE_DPAD_LEFT},
            {"↓", KeyEvent.KEYCODE_DPAD_DOWN}, {"→", KeyEvent.KEYCODE_DPAD_RIGHT},
            {"PGDN", KeyEvent.KEYCODE_PAGE_DOWN}, {"⌨", TYPING}},
    };
    /** How long a key's held before it repeats, and then how often. */
    private static final long REPEAT_AFTER_MS = 400, REPEAT_EVERY_MS = 50;
    /** A sticky key's two taps, as one double tap. */
    private static final long DOUBLE_TAP_MS = 300;

    private static final int KEY = 0xff1b1b1f, KEY_DOWN = 0xff3a3a44, LIT = 0xffff2a9d, TEXT = 0xffe8eaed;

    /** A sticky key's state: off, for the next key only, or held till it's tapped again. */
    private enum Sticky { OFF, ONCE, HELD }

    private final Handler handler = new Handler(Looper.getMainLooper());
    private final KeyCharacterMap characters = KeyCharacterMap.load(KeyCharacterMap.VIRTUAL_KEYBOARD);
    private final Typing typing;
    private final float dp;
    private Sticky ctrl = Sticky.OFF, alt = Sticky.OFF;
    private TextView ctrlKey, altKey;
    private long ctrlTapped, altTapped;

    Keyboard(Context context) {
        super(context);
        setOrientation(VERTICAL);
        setBackgroundColor(0xff0d0d10);
        dp = context.getResources().getDisplayMetrics().density;
        for (Object[][] row : ROWS) {
            LinearLayout keys = new LinearLayout(context);
            keys.setOrientation(HORIZONTAL);
            for (Object[] key : row) {
                keys.addView(key((String) key[0], (Integer) key[1]),
                        new LayoutParams(0, (int) (46 * dp), 1));
            }
            addView(keys, new LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT));
        }
        // Android's keyboard's, unseen: where what it types comes in.
        typing = new Typing(context);
        addView(typing, new LayoutParams(1, 1));
    }

    /** Android's keyboard up, to type on. */
    void showTyping() {
        typing.requestFocus();
        // Once it's laid out and focused, or Android ignores the asking.
        handler.postDelayed(() -> {
            typing.requestFocus();
            input().showSoftInput(typing, 0);
        }, 150);
    }

    /** Android's keyboard put away. */
    void hideTyping() {
        input().hideSoftInputFromWindow(typing.getWindowToken(), 0);
    }

    private InputMethodManager input() {
        return getContext().getSystemService(InputMethodManager.class);
    }

    /** A key of the rows. */
    private TextView key(String label, int code) {
        TextView key = new TextView(getContext());
        key.setText(label);
        key.setTextColor(TEXT);
        key.setTextSize(TypedValue.COMPLEX_UNIT_SP, label.length() > 1 && !label.equals("⌨") ? 14 : 20);
        key.setGravity(Gravity.CENTER);
        key.setBackground(face(KEY));
        key.setHapticFeedbackEnabled(true);
        if (code == CTRL) {
            ctrlKey = key;
            key.setOnClickListener(v -> {
                ctrl = tapped(ctrl, ctrlTapped);
                ctrlTapped = SystemClock.uptimeMillis();
                light();
            });
        } else if (code == ALT) {
            altKey = key;
            key.setOnClickListener(v -> {
                alt = tapped(alt, altTapped);
                altTapped = SystemClock.uptimeMillis();
                light();
            });
        } else if (code == TYPING) {
            key.setOnClickListener(v -> {
                if (typingShown) {
                    hideTyping();
                } else {
                    showTyping();
                }
            });
        }
        key.setOnTouchListener(code < 0 ? tick() : new Repeating(code));
        return key;
    }

    /** Whether Android's keyboard's up, as its insets last said. */
    private boolean typingShown;

    void typingShown(boolean shown) {
        typingShown = shown;
    }

    /** A sticky key tapped: off to once, once to held if tapped again quickly, else off. */
    private static Sticky tapped(Sticky was, long last) {
        boolean twice = SystemClock.uptimeMillis() - last < DOUBLE_TAP_MS;
        switch (was) {
            case OFF:
                return Sticky.ONCE;
            case ONCE:
                return twice ? Sticky.HELD : Sticky.OFF;
            default:
                return Sticky.OFF;
        }
    }

    /** The sticky keys lit as they're on: held brighter than once. */
    private void light() {
        ctrlKey.setBackground(face(ctrl == Sticky.OFF ? KEY : LIT));
        ctrlKey.setAlpha(ctrl == Sticky.ONCE ? 0.75f : 1);
        altKey.setBackground(face(alt == Sticky.OFF ? KEY : LIT));
        altKey.setAlpha(alt == Sticky.ONCE ? 0.75f : 1);
    }

    private GradientDrawable face(int colour) {
        GradientDrawable face = new GradientDrawable();
        face.setColor(colour);
        face.setCornerRadius(6 * dp);
        face.setStroke((int) (2 * dp), 0xff0d0d10);
        return face;
    }

    /** A sticky key's or ⌨'s tap ticked, and clicked as it's let go. */
    private View.OnTouchListener tick() {
        return (view, event) -> {
            if (event.getActionMasked() == MotionEvent.ACTION_DOWN) {
                view.performHapticFeedback(HapticFeedbackConstants.VIRTUAL_KEY);
                view.setPressed(true);
            } else if (event.getActionMasked() == MotionEvent.ACTION_UP) {
                view.setPressed(false);
                view.performClick();
            } else if (event.getActionMasked() == MotionEvent.ACTION_CANCEL) {
                view.setPressed(false);
            }
            return true;
        };
    }

    /**
     * A key of the rows pressed as long as it's touched, repeating after a
     * moment, as a keyboard's does; with the sticky keys that are on.
     */
    private class Repeating implements View.OnTouchListener {
        private final int code;
        private int repeats;
        private final Runnable again = new Runnable() {
            @Override
            public void run() {
                repeats++;
                down(code, modifiers(), repeats);
                handler.postDelayed(this, REPEAT_EVERY_MS);
            }
        };
        private int held;

        Repeating(int code) {
            this.code = code;
        }

        @Override
        public boolean onTouch(View view, MotionEvent event) {
            switch (event.getActionMasked()) {
                case MotionEvent.ACTION_DOWN:
                    view.performHapticFeedback(HapticFeedbackConstants.VIRTUAL_KEY);
                    view.setBackground(face(KEY_DOWN));
                    held = modifiers();
                    pressModifiers(held, true);
                    repeats = 0;
                    down(code, held, 0);
                    handler.postDelayed(again, REPEAT_AFTER_MS);
                    return true;
                case MotionEvent.ACTION_UP:
                case MotionEvent.ACTION_CANCEL:
                    handler.removeCallbacks(again);
                    view.setBackground(face(KEY));
                    up(code, held);
                    pressModifiers(held, false);
                    used();
                    return true;
                default:
                    return true;
            }
        }
    }

    /** The sticky keys that are on, as a key event's meta state. */
    private int modifiers() {
        int meta = 0;
        if (ctrl != Sticky.OFF) {
            meta |= KeyEvent.META_CTRL_ON | KeyEvent.META_CTRL_LEFT_ON;
        }
        if (alt != Sticky.OFF) {
            meta |= KeyEvent.META_ALT_ON | KeyEvent.META_ALT_LEFT_ON;
        }
        return meta;
    }

    /** A key's been pressed with the sticky keys: those on for it once, off. */
    private void used() {
        if (ctrl == Sticky.ONCE) {
            ctrl = Sticky.OFF;
        }
        if (alt == Sticky.ONCE) {
            alt = Sticky.OFF;
        }
        light();
    }

    /** Ctrl and Alt pressed, or let go, around a key, as `meta` has them. */
    private void pressModifiers(int meta, boolean down) {
        if ((meta & KeyEvent.META_CTRL_ON) != 0) {
            send(down, KeyEvent.KEYCODE_CTRL_LEFT, meta, 0);
        }
        if ((meta & KeyEvent.META_ALT_ON) != 0) {
            send(down, KeyEvent.KEYCODE_ALT_LEFT, meta, 0);
        }
    }

    private void down(int code, int meta, int repeat) {
        send(true, code, meta, repeat);
    }

    private void up(int code, int meta) {
        send(false, code, meta, 0);
    }

    /**
     * A key event to the game, as a hardware keyboard's goes: pressed, and
     * the text it types if it types any.
     */
    private void send(boolean down, int code, int meta, int repeat) {
        long now = SystemClock.uptimeMillis();
        KeyEvent event = new KeyEvent(now, now, down ? KeyEvent.ACTION_DOWN : KeyEvent.ACTION_UP,
                code, repeat, meta, KeyCharacterMap.VIRTUAL_KEYBOARD, 0, 0);
        dispatch(event);
    }

    private static void dispatch(KeyEvent event) {
        SDLActivity.handleKeyEvent(null, event.getKeyCode(), event, null);
    }

    /**
     * Characters typed on Android's keyboard, each as its keys: Shift
     * pressed for a capital, as a keyboard has it; with the sticky keys.
     * One no keyboard has (an accented letter, say) is left out.
     */
    private void type(CharSequence text) {
        for (int at = 0; at < text.length(); at++) {
            char c = text.charAt(at);
            if (c == '\n') {
                stroke(KeyEvent.KEYCODE_ENTER);
                continue;
            }
            KeyEvent[] events = characters.getEvents(new char[] {c});
            if (events == null) {
                continue;
            }
            int meta = modifiers();
            pressModifiers(meta, true);
            for (KeyEvent event : events) {
                dispatch(meta == 0 ? event : new KeyEvent(event.getDownTime(), event.getEventTime(),
                        event.getAction(), event.getKeyCode(), event.getRepeatCount(),
                        event.getMetaState() | meta, event.getDeviceId(), event.getScanCode()));
            }
            pressModifiers(meta, false);
            used();
        }
    }

    /** A key pressed and let go, with the sticky keys. */
    private void stroke(int code) {
        int meta = modifiers();
        pressModifiers(meta, true);
        down(code, meta, 0);
        up(code, meta);
        pressModifiers(meta, false);
        used();
    }

    /**
     * Where Android's keyboard types: unseen, taking its focus. Its text
     * typed as keys; its Backspace, Return and the rest, keys; a hardware
     * keyboard's keys, while it has the focus, passed on to the game.
     */
    private class Typing extends View {
        Typing(Context context) {
            super(context);
            setFocusable(true);
            setFocusableInTouchMode(true);
        }

        @Override
        public boolean onCheckIsTextEditor() {
            return true;
        }

        @Override
        public InputConnection onCreateInputConnection(EditorInfo info) {
            info.inputType = InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD
                    | InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS;
            info.imeOptions = EditorInfo.IME_FLAG_NO_FULLSCREEN | EditorInfo.IME_FLAG_NO_EXTRACT_UI
                    | EditorInfo.IME_ACTION_NONE;
            return new BaseInputConnection(this, false) {
                /** What a keyboard that composes has typed of the word it's composing. */
                private CharSequence composing = "";

                @Override
                public boolean commitText(CharSequence text, int newCursorPosition) {
                    retype(text);
                    composing = "";
                    return true;
                }

                @Override
                public boolean setComposingText(CharSequence text, int newCursorPosition) {
                    retype(text);
                    composing = text;
                    return true;
                }

                @Override
                public boolean finishComposingText() {
                    composing = "";
                    return true;
                }

                /** `text` in place of what's being composed: what's changed of it typed again. */
                private void retype(CharSequence text) {
                    int same = 0;
                    while (same < composing.length() && same < text.length()
                            && composing.charAt(same) == text.charAt(same)) {
                        same++;
                    }
                    for (int n = same; n < composing.length(); n++) {
                        stroke(KeyEvent.KEYCODE_DEL);
                    }
                    type(text.subSequence(same, text.length()));
                }

                @Override
                public boolean deleteSurroundingText(int before, int after) {
                    for (int n = 0; n < before; n++) {
                        stroke(KeyEvent.KEYCODE_DEL);
                    }
                    for (int n = 0; n < after; n++) {
                        stroke(KeyEvent.KEYCODE_FORWARD_DEL);
                    }
                    return true;
                }

                @Override
                public boolean sendKeyEvent(KeyEvent event) {
                    if (event.getAction() == KeyEvent.ACTION_DOWN) {
                        stroke(event.getKeyCode());
                    }
                    return true;
                }
            };
        }

        @Override
        public boolean onKeyDown(int keyCode, KeyEvent event) {
            return SDLActivity.handleKeyEvent(this, keyCode, event, null);
        }

        @Override
        public boolean onKeyUp(int keyCode, KeyEvent event) {
            return SDLActivity.handleKeyEvent(this, keyCode, event, null);
        }
    }
}
