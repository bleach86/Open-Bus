package app.openbus;

import android.content.Context;
import android.content.SharedPreferences;
import android.graphics.Canvas;
import android.graphics.DashPathEffect;
import android.graphics.Paint;
import android.graphics.Path;
import android.graphics.Point;
import android.graphics.PointF;
import android.graphics.RectF;
import android.util.SparseArray;
import android.util.SparseBooleanArray;
import android.view.HapticFeedbackConstants;
import android.view.MotionEvent;
import android.view.View;
import android.view.WindowManager;

/**
 * A touch screen's gamepad, over the game: a D-pad, A and B as a NES pad
 * has them, Select and Start; or, for a machine with a SNES pad (the
 * Commander X16), A, B, X and Y in its diamond, and its shoulders, L and R,
 * at the top corners, laid out and kept apart from the NES's. Drawn by Android, at the screen's own
 * resolution, sized in dp, as any app's buttons are. As it comes, the
 * D-pad's in the middle of the bar left of the picture, A and B in the
 * middle of the right one, Select and Start under them; each where it's
 * been moved to, and all as big as they've been made, as they're kept.
 * Any number of fingers at once; one that lands on the D-pad keeps it
 * however far it slides, and one between A and B holds both. What's held
 * is told as it changes, by the bits of the emulator's touch module, each
 * new press a tick of the phone's touch feedback, as its settings have it,
 * unless it's been turned off. A higher than B, as a NES pad's are, or B
 * higher, as it's been set. Being moved, it holds nothing: each piece goes
 * where it's dragged. Each piece is placed on the screen itself, not in the
 * game's window: as Android's bars come up over it, and the window's
 * narrowed or moved for them, they stay where they are. A finger that
 * lands on none of it, on the picture, is passed on, as a touch of the
 * picture's: a light gun's, aimed, or a mouse's trackpad.
 */
public class TouchPad extends View {
    /** As the emulator's touch module has them. */
    static final int A = 1, B = 1 << 1, SELECT = 1 << 2, START = 1 << 3;
    static final int UP = 1 << 4, DOWN = 1 << 5, LEFT = 1 << 6, RIGHT = 1 << 7;
    static final int X = 1 << 8, Y = 1 << 9, L = 1 << 10, R = 1 << 11;

    /** What's told what's held. */
    interface Held {
        void held(int buttons);
    }

    /** What's told where the stick's pushed, across and down, -1 to 1, as it's held, and let go. */
    interface Steered {
        void steered(float x, float y, boolean held);
    }

    /** Its kinds: a NES pad; a SNES pad; a joystick's stick and two buttons, an Apple II's. */
    static final int NES = 0, SNES = 1, STICK = 2;

    /** What's passed a finger that landed on none of it: a touch, as a MotionEvent's action, where on the screen. */
    interface Passed {
        void passed(int device, int finger, int action, float x, float y);
    }

    /** Where it's kept: each piece's place, a share of the screen across and down; and its size. */
    private static final String KEPT = "touch_pad", KEPT_SNES = "touch_pad_snes", KEPT_STICK = "touch_pad_stick";
    private static final String SCALE = "scale";
    private static final String TICKS = "haptics", B_HIGHER = "b_higher";
    private static final String[] PIECES = {"dpad", "buttons", "select", "start", "l", "r"};
    private static final int DPAD = 0, BUTTONS = 1, SELECT_PILL = 2, START_PILL = 3, L_PILL = 4, R_PILL = 5;
    static final float SMALLEST = 0.6f, BIGGEST = 1.8f;

    // Its measures, in dp, as big as it comes.
    private static final float MARGIN = 16, PAD = 70, ARM = 24, BUTTON = 31;
    private static final float BUTTONS_APART = 37, BUTTONS_STAGGER = 20;
    private static final float PILL_WIDTH = 66, PILL_HEIGHT = 26, PILLS_BELOW = 46;
    /** A SNES pad's: its four buttons' distance from their middle, and their size; its shoulders'. */
    private static final float DIAMOND = 38, FACE = 26, SHOULDER_WIDTH = 84, SHOULDER_HEIGHT = 32;
    /** The picture's shape, the NES's, as wide as it's tall, near enough: where the bars are. */
    private static final float PICTURE = 1.31f;
    /** How far past a button's edge a finger still has it, and the D-pad's middle, where it's nothing. */
    private static final float SLOP = 1.3f, DEAD = 0.2f;
    /** How far off an axis a finger on the D-pad still holds that direction alone: 30 degrees. */
    private static final float DIAGONAL = 0.5f;

    private final Held told;
    private final SharedPreferences kept;
    private final boolean moving, snes, stick;
    private final int kind;
    /** The stick, pushed as its finger is, across and down, -1 to 1; and whether it's held. */
    private float pushedX, pushedY;
    private boolean steering;
    private Steered steered = (x, y, held) -> {};
    private final float dp;
    private final Paint fill = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint pressed = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint edge = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint label = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint outline = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Path cross = new Path();
    private final Path arrow = new Path();
    private final RectF arm = new RectF(), box = new RectF();
    /** Each piece's middle: a SNES pad's six, a NES pad's first four. */
    private final PointF[] at;
    /** The fingers that landed on the D-pad, by their pointer ids. */
    private final SparseBooleanArray onDpad = new SparseBooleanArray();
    /** The fingers that landed on none of it, passed on, by their pointer ids, and where each was last. */
    private final SparseArray<PointF> passing = new SparseArray<>();
    private Passed passed = (device, finger, action, x, y) -> {};
    private int device;

    /** The screen's size, and where the view is on it. */
    private int screenWidth, screenHeight;
    private final int[] onScreen = new int[2];

    private float scale;
    private boolean ticks, bHigher;
    private int held;
    /** Being moved: the piece that's dragged, or -1, and how far the finger's off its middle. */
    private int dragged = -1;
    private float dragX, dragY;

    /**
     * As it's played, `told` what's held; or, `moving`, to be moved and
     * resized, telling nothing. Of its `kind`: NES, SNES or STICK.
     */
    TouchPad(Context context, Held told, boolean moving, int kind) {
        super(context);
        this.told = told;
        this.moving = moving;
        this.kind = kind;
        this.snes = kind == SNES;
        this.stick = kind == STICK;
        // A stick's two pieces, the stick and its buttons; a NES pad's four.
        at = new PointF[snes ? PIECES.length : stick ? 2 : 4];
        for (int piece = 0; piece < at.length; piece++) {
            at[piece] = new PointF();
        }
        kept = context.getSharedPreferences(snes ? KEPT_SNES : stick ? KEPT_STICK : KEPT, Context.MODE_PRIVATE);
        dp = context.getResources().getDisplayMetrics().density;
        read();
        fill.setColor(0x38ffffff);
        pressed.setColor(0x90ffffff);
        edge.setColor(0x99ffffff);
        edge.setStyle(Paint.Style.STROKE);
        edge.setStrokeJoin(Paint.Join.ROUND);
        label.setColor(0xccffffff);
        label.setTextAlign(Paint.Align.CENTER);
        label.setFakeBoldText(true);
        outline.setColor(0xffffd54f);
        outline.setStyle(Paint.Style.STROKE);
        outline.setStrokeWidth(2 * dp);
        outline.setPathEffect(new DashPathEffect(new float[] {8 * dp, 6 * dp}, 0));
        setFocusable(false);
        setHapticFeedbackEnabled(true);
    }

    /** All let go, as it's hidden or the game's paused. */
    void letGo() {
        onDpad.clear();
        for (int n = 0; n < passing.size(); n++) {
            PointF last = passing.valueAt(n);
            passed.passed(device, passing.keyAt(n), MotionEvent.ACTION_UP, last.x, last.y);
        }
        passing.clear();
        steer(0, 0, false);
        tell(0);
    }

    /** What's passed the fingers that land on none of it. */
    void passTo(Passed passed) {
        this.passed = passed;
    }

    /** Where it's kept and how big, read again: as it's been moved. */
    void reload() {
        read();
        place();
        invalidate();
    }

    private void read() {
        scale = kept.getFloat(SCALE, 1);
        ticks = kept.getBoolean(TICKS, true);
        bHigher = kept.getBoolean(B_HIGHER, false);
    }

    /** Whether each press ticks. */
    boolean ticks() {
        return ticks;
    }

    /** Each press's tick turned on or off, and kept. */
    void ticks(boolean on) {
        ticks = on;
        kept.edit().putBoolean(TICKS, on).apply();
    }

    /** Its kind: NES, SNES or STICK. */
    int kind() {
        return kind;
    }

    /** What's told where the stick's pushed. */
    void steerTo(Steered steered) {
        this.steered = steered;
    }

    /** Whether B's higher than A. */
    boolean bHigher() {
        return bHigher;
    }

    /** B made higher than A, or A than B, and kept. */
    void bHigher(boolean higher) {
        bHigher = higher;
        kept.edit().putBoolean(B_HIGHER, higher).apply();
        invalidate();
    }

    /** Made bigger or smaller by `by`, within bounds, and kept. */
    void resize(float by) {
        scale = Math.max(SMALLEST, Math.min(BIGGEST, scale + by));
        for (int piece = 0; piece < at.length; piece++) {
            clamp(piece);
        }
        keep();
        shape();
        invalidate();
    }

    /** As it came: its places and its size forgotten. */
    void reset() {
        kept.edit().clear().apply();
        reload();
    }

    /** dp, at its size. */
    private float u() {
        return dp * scale;
    }

    @Override
    protected void onSizeChanged(int width, int height, int oldWidth, int oldHeight) {
        Point size = new Point();
        getContext().getSystemService(WindowManager.class).getDefaultDisplay().getRealSize(size);
        screenWidth = size.x;
        screenHeight = size.y;
        place();
    }

    @Override
    protected void onLayout(boolean changed, int left, int top, int right, int bottom) {
        super.onLayout(changed, left, top, right, bottom);
        invalidate();
    }

    /** Each piece where it's kept, or where it comes. */
    private void place() {
        int width = screenWidth, height = screenHeight;
        if (width == 0 || height == 0) {
            return;
        }
        float u = u(), margin = MARGIN * dp;
        // The middle of each bar beside the picture, or as near as fits.
        float bar = Math.max(0, (width - height * PICTURE) / 2);
        float buttons = snes ? (DIAMOND + FACE) * u : (BUTTONS_APART + BUTTON) * u;
        float left = Math.max(bar / 2, margin + PAD * u);
        float right = width - Math.max(bar / 2, margin + buttons);
        float middle = height / 2f;
        float pills = middle + PAD * u + PILLS_BELOW * u;
        float shoulders = margin + SHOULDER_HEIGHT * u / 2;
        float[][] comes = {
            {left, middle}, {right, middle}, {left, pills}, {right, pills}, {left, shoulders}, {right, shoulders}
        };
        for (int piece = 0; piece < at.length; piece++) {
            String name = PIECES[piece];
            at[piece].set(kept.getFloat(name + "_x", -1) >= 0 ? kept.getFloat(name + "_x", 0) * width : comes[piece][0],
                    kept.getFloat(name + "_y", -1) >= 0 ? kept.getFloat(name + "_y", 0) * height : comes[piece][1]);
            clamp(piece);
        }
        shape();
    }

    /** Each piece's place and the size kept. */
    private void keep() {
        SharedPreferences.Editor editor = kept.edit().putFloat(SCALE, scale);
        for (int piece = 0; piece < at.length; piece++) {
            editor.putFloat(PIECES[piece] + "_x", at[piece].x / screenWidth);
            editor.putFloat(PIECES[piece] + "_y", at[piece].y / screenHeight);
        }
        editor.apply();
    }

    /** `piece` kept wholly on the screen. */
    private void clamp(int piece) {
        bounds(piece, box);
        PointF point = at[piece];
        point.x += Math.max(0, -box.left) - Math.max(0, box.right - screenWidth);
        point.y += Math.max(0, -box.top) - Math.max(0, box.bottom - screenHeight);
    }

    /** What `piece` covers, into `into`. */
    private void bounds(int piece, RectF into) {
        float u = u(), halfWidth, halfHeight;
        switch (piece) {
            case DPAD:
                halfWidth = halfHeight = PAD * u;
                break;
            case BUTTONS:
                halfWidth = (snes ? DIAMOND + FACE : BUTTONS_APART + BUTTON) * u;
                halfHeight = (snes ? DIAMOND + FACE : BUTTONS_STAGGER + BUTTON) * u;
                break;
            case L_PILL:
            case R_PILL:
                halfWidth = SHOULDER_WIDTH * u / 2;
                halfHeight = SHOULDER_HEIGHT * u / 2;
                break;
            default:
                halfWidth = PILL_WIDTH * u / 2;
                halfHeight = PILL_HEIGHT * u / 2;
                break;
        }
        PointF point = at[piece];
        into.set(point.x - halfWidth, point.y - halfHeight, point.x + halfWidth, point.y + halfHeight);
    }

    /** The D-pad's cross, its two bars as one shape, where it is. */
    private void shape() {
        float u = u(), pad = PAD * u, half = ARM * u, round = 6 * u;
        PointF dpad = at[DPAD];
        Path across = new Path(), down = new Path();
        across.addRoundRect(dpad.x - pad, dpad.y - half, dpad.x + pad, dpad.y + half, round, round, Path.Direction.CW);
        down.addRoundRect(dpad.x - half, dpad.y - pad, dpad.x + half, dpad.y + pad, round, round, Path.Direction.CW);
        cross.reset();
        cross.op(across, down, Path.Op.UNION);
        edge.setStrokeWidth(2 * dp);
    }

    private float aX() {
        return at[BUTTONS].x + BUTTONS_APART * u();
    }

    private float aY() {
        return at[BUTTONS].y + (bHigher ? 1 : -1) * BUTTONS_STAGGER * u();
    }

    private float bX() {
        return at[BUTTONS].x - BUTTONS_APART * u();
    }

    private float bY() {
        return at[BUTTONS].y - (bHigher ? 1 : -1) * BUTTONS_STAGGER * u();
    }

    @Override
    protected void onDraw(Canvas canvas) {
        // Where it is on the screen, its pieces where they are on it.
        getLocationOnScreen(onScreen);
        canvas.save();
        canvas.translate(-onScreen[0], -onScreen[1]);
        drawPieces(canvas);
        canvas.restore();
    }

    private void drawPieces(Canvas canvas) {
        if (stick) {
            drawStick(canvas);
            return;
        }
        float u = u(), pad = PAD * u, half = ARM * u, round = 6 * u;
        float x = at[DPAD].x, y = at[DPAD].y;
        canvas.drawPath(cross, fill);
        // Each held direction's arm lit, from the middle out.
        if ((held & UP) != 0) {
            arm.set(x - half, y - pad, x + half, y - half);
            canvas.drawRoundRect(arm, round, round, pressed);
        }
        if ((held & DOWN) != 0) {
            arm.set(x - half, y + half, x + half, y + pad);
            canvas.drawRoundRect(arm, round, round, pressed);
        }
        if ((held & LEFT) != 0) {
            arm.set(x - pad, y - half, x - half, y + half);
            canvas.drawRoundRect(arm, round, round, pressed);
        }
        if ((held & RIGHT) != 0) {
            arm.set(x + half, y - half, x + pad, y + half);
            canvas.drawRoundRect(arm, round, round, pressed);
        }
        canvas.drawPath(cross, edge);
        // An arrow in each arm, pointing out.
        float tip = pad - 12 * u, base = pad - 30 * u, wide = 10 * u;
        for (int turn = 0; turn < 4; turn++) {
            canvas.save();
            canvas.rotate(90 * turn, x, y);
            arrow.reset();
            arrow.moveTo(x + tip, y);
            arrow.lineTo(x + base, y - wide);
            arrow.lineTo(x + base, y + wide);
            arrow.close();
            canvas.drawPath(arrow, label);
            canvas.restore();
        }

        if (snes) {
            float apart = DIAMOND * u, middleX = at[BUTTONS].x, middleY = at[BUTTONS].y;
            button(canvas, middleX + apart, middleY, "A", (held & A) != 0, FACE);
            button(canvas, middleX, middleY + apart, "B", (held & B) != 0, FACE);
            button(canvas, middleX, middleY - apart, "X", (held & X) != 0, FACE);
            button(canvas, middleX - apart, middleY, "Y", (held & Y) != 0, FACE);
            pill(canvas, L_PILL, "L", (held & L) != 0);
            pill(canvas, R_PILL, "R", (held & R) != 0);
        } else {
            button(canvas, aX(), aY(), "A", (held & A) != 0, BUTTON);
            button(canvas, bX(), bY(), "B", (held & B) != 0, BUTTON);
        }
        pill(canvas, SELECT_PILL, "SELECT", (held & SELECT) != 0);
        pill(canvas, START_PILL, "START", (held & START) != 0);
        outlined(canvas);
    }

    /**
     * A joystick's: its stick, a knob in a ring, the knob where it's pushed;
     * and its two buttons where a NES pad's A and B are, 0 and 1, as an
     * Apple II's are numbered.
     */
    private void drawStick(Canvas canvas) {
        float u = u(), ring = PAD * u, knob = PAD * u * 0.42f;
        float x = at[DPAD].x, y = at[DPAD].y;
        canvas.drawCircle(x, y, ring, fill);
        canvas.drawCircle(x, y, ring, edge);
        float knobX = x + pushedX * (ring - knob), knobY = y + pushedY * (ring - knob);
        canvas.drawCircle(knobX, knobY, knob, steering ? pressed : fill);
        canvas.drawCircle(knobX, knobY, knob, edge);
        button(canvas, aX(), aY(), "0", (held & A) != 0, BUTTON);
        button(canvas, bX(), bY(), "1", (held & B) != 0, BUTTON);
        outlined(canvas);
    }

    /** Being moved: each piece outlined, to be seen as one to drag. */
    private void outlined(Canvas canvas) {
        if (moving) {
            for (int piece = 0; piece < at.length; piece++) {
                bounds(piece, box);
                box.inset(-6 * dp, -6 * dp);
                canvas.drawRoundRect(box, 8 * dp, 8 * dp, outline);
            }
        }
    }

    private void button(Canvas canvas, float x, float y, String name, boolean down, float size) {
        float radius = size * u();
        canvas.drawCircle(x, y, radius, down ? pressed : fill);
        canvas.drawCircle(x, y, radius, edge);
        label.setTextSize(22 * u());
        canvas.drawText(name, x, y - (label.ascent() + label.descent()) / 2, label);
    }

    private void pill(Canvas canvas, int piece, String name, boolean down) {
        bounds(piece, box);
        float round = box.height() / 2;
        canvas.drawRoundRect(box, round, round, down ? pressed : fill);
        canvas.drawRoundRect(box, round, round, edge);
        label.setTextSize((name.length() == 1 ? 15 : 11) * u());
        canvas.drawText(name, box.centerX(), box.centerY() - (label.ascent() + label.descent()) / 2, label);
    }

    @Override
    public boolean onTouchEvent(MotionEvent event) {
        getLocationOnScreen(onScreen);
        if (moving) {
            return move(event);
        }
        int action = event.getActionMasked();
        device = event.getDeviceId();
        if (action == MotionEvent.ACTION_CANCEL) {
            letGo();
            return true;
        }
        // The finger lifted, as this event has it, left out of what's held.
        int lifted = action == MotionEvent.ACTION_UP || action == MotionEvent.ACTION_POINTER_UP
                ? event.getActionIndex() : -1;
        if (action == MotionEvent.ACTION_DOWN || action == MotionEvent.ACTION_POINTER_DOWN) {
            int landed = event.getActionIndex();
            float x = event.getX(landed) + onScreen[0], y = event.getY(landed) + onScreen[1];
            int finger = event.getPointerId(landed);
            onDpad.put(finger, onDpad(x, y));
            if (!onDpad.get(finger) && buttons(x, y) == 0) {
                passing.put(finger, new PointF(x, y));
                passed.passed(device, finger, MotionEvent.ACTION_DOWN, x, y);
            }
        } else if (action == MotionEvent.ACTION_MOVE) {
            for (int n = 0; n < event.getPointerCount(); n++) {
                PointF last = passing.get(event.getPointerId(n));
                if (last != null) {
                    last.set(event.getX(n) + onScreen[0], event.getY(n) + onScreen[1]);
                    passed.passed(device, event.getPointerId(n), MotionEvent.ACTION_MOVE, last.x, last.y);
                }
            }
        }
        int buttons = 0;
        boolean onStick = false;
        for (int n = 0; n < event.getPointerCount(); n++) {
            if (n == lifted) {
                continue;
            }
            float x = event.getX(n) + onScreen[0], y = event.getY(n) + onScreen[1];
            if (stick && onDpad.get(event.getPointerId(n))) {
                onStick = true;
                push(x, y);
            } else {
                buttons |= onDpad.get(event.getPointerId(n)) ? directions(x, y) : buttons(x, y);
            }
        }
        if (stick && !onStick && steering) {
            steer(0, 0, false);
        }
        if (lifted >= 0) {
            int finger = event.getPointerId(lifted);
            onDpad.delete(finger);
            if (passing.get(finger) != null) {
                passed.passed(device, finger, MotionEvent.ACTION_UP,
                        event.getX(lifted) + onScreen[0], event.getY(lifted) + onScreen[1]);
                passing.delete(finger);
            }
        }
        tell(buttons);
        return true;
    }

    /** Being moved: the piece a finger lands on dragged with it, one at a time, and kept as it's let go. */
    private boolean move(MotionEvent event) {
        float x = event.getX() + onScreen[0], y = event.getY() + onScreen[1];
        switch (event.getActionMasked()) {
            case MotionEvent.ACTION_DOWN:
                dragged = -1;
                float slop = 12 * dp;
                for (int piece = at.length - 1; piece >= 0; piece--) {
                    bounds(piece, box);
                    box.inset(-slop, -slop);
                    if (box.contains(x, y)) {
                        dragged = piece;
                        dragX = x - at[piece].x;
                        dragY = y - at[piece].y;
                        tick();
                        break;
                    }
                }
                break;
            case MotionEvent.ACTION_MOVE:
                if (dragged >= 0) {
                    at[dragged].set(x - dragX, y - dragY);
                    clamp(dragged);
                    if (dragged == DPAD) {
                        shape();
                    }
                    invalidate();
                }
                break;
            case MotionEvent.ACTION_UP:
            case MotionEvent.ACTION_CANCEL:
                if (dragged >= 0) {
                    keep();
                }
                dragged = -1;
                break;
            default:
                break;
        }
        return true;
    }

    /**
     * The stick pushed by a finger at `x`, `y`: by how far it is from the
     * middle, all the way at most of the stick's reach, and nothing near it.
     */
    private void push(float x, float y) {
        float reach = PAD * u() * 0.8f;
        float across = (x - at[DPAD].x) / reach, down = (y - at[DPAD].y) / reach;
        float away = (float) Math.hypot(across, down);
        if (away < DEAD * 0.75f) {
            across = down = 0;
        } else if (away > 1) {
            across /= away;
            down /= away;
        }
        steer(across, down, true);
    }

    /** The stick told where it's pushed, if that's changed, and drawn there. */
    private void steer(float x, float y, boolean held) {
        if (x == pushedX && y == pushedY && held == steering) {
            return;
        }
        if (held && !steering) {
            tick();
        }
        pushedX = x;
        pushedY = y;
        steering = held;
        steered.steered(x, y, held);
        invalidate();
    }

    /** Whether a finger landing at `x`, `y` lands on the D-pad. */
    private boolean onDpad(float x, float y) {
        return Math.hypot(x - at[DPAD].x, y - at[DPAD].y) <= PAD * u() * SLOP;
    }

    /**
     * The directions a finger on the D-pad holds: its way from the middle,
     * one alone within 30 degrees of it, two between; nothing in the middle.
     */
    private int directions(float x, float y) {
        float dx = x - at[DPAD].x, dy = y - at[DPAD].y;
        double away = Math.hypot(dx, dy);
        if (away < PAD * u() * DEAD) {
            return 0;
        }
        double across = dx / away, down = dy / away;
        int held = 0;
        if (across > DIAGONAL) {
            held |= RIGHT;
        } else if (across < -DIAGONAL) {
            held |= LEFT;
        }
        if (down > DIAGONAL) {
            held |= DOWN;
        } else if (down < -DIAGONAL) {
            held |= UP;
        }
        return held;
    }

    /** The buttons a finger at `x`, `y` holds, not on the D-pad. */
    private int buttons(float x, float y) {
        int pills = pills(x, y);
        if (pills != 0) {
            return pills;
        }
        return snes ? diamond(x, y) : aAndB(x, y);
    }

    /**
     * A SNES pad's buttons a finger at `x`, `y` holds: the one it's on, or
     * nearest, within reach; or, between two side by side, both, as a thumb
     * across them holds both on a real one.
     */
    private int diamond(float x, float y) {
        float u = u(), apart = DIAMOND * u, reach = FACE * u * SLOP;
        float dx = x - at[BUTTONS].x, dy = y - at[BUTTONS].y;
        float[][] faces = {{apart, 0}, {0, apart}, {0, -apart}, {-apart, 0}};
        int[] bits = {A, B, X, Y};
        int nearest = -1;
        double best = reach;
        for (int face = 0; face < faces.length; face++) {
            double away = Math.hypot(dx - faces[face][0], dy - faces[face][1]);
            if (away <= best) {
                best = away;
                nearest = face;
            }
        }
        if (nearest < 0) {
            return 0;
        }
        // Halfway to a neighbour, that one too.
        int held = bits[nearest];
        for (int face = 0; face < faces.length; face++) {
            float midX = (faces[face][0] + faces[nearest][0]) / 2, midY = (faces[face][1] + faces[nearest][1]) / 2;
            if (face != nearest && (midX != 0 || midY != 0)
                    && Math.hypot(dx - midX, dy - midY) < FACE * u * 0.5f) {
                held |= bits[face];
            }
        }
        return held;
    }

    /** A NES pad's A and B a finger at `x`, `y` holds. */
    private int aAndB(float x, float y) {
        float u = u(), reach = BUTTON * u * SLOP;
        // Between A and B, both.
        if (Math.hypot(x - at[BUTTONS].x, y - at[BUTTONS].y) < BUTTON * u * 0.6f) {
            return A | B;
        }
        if (Math.hypot(x - aX(), y - aY()) <= reach) {
            return A;
        }
        if (Math.hypot(x - bX(), y - bY()) <= reach) {
            return B;
        }
        return 0;
    }

    /** The pill a finger at `x`, `y` is on, Select, Start or a shoulder, or 0. */
    private int pills(float x, float y) {
        float slop = 10 * dp;
        int[] bits = {0, 0, SELECT, START, L, R};
        for (int piece = SELECT_PILL; piece < at.length; piece++) {
            bounds(piece, box);
            box.inset(-slop, -slop);
            if (box.contains(x, y)) {
                return bits[piece];
            }
        }
        return 0;
    }

    private void tick() {
        if (ticks) {
            performHapticFeedback(HapticFeedbackConstants.VIRTUAL_KEY);
        }
    }

    private void tell(int buttons) {
        if (buttons == held) {
            return;
        }
        if ((buttons & ~held) != 0) {
            tick();
        }
        held = buttons;
        told.held(buttons);
        invalidate();
    }
}
