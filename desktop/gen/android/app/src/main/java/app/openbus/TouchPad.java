package app.openbus;

import android.content.Context;
import android.graphics.Canvas;
import android.graphics.Paint;
import android.graphics.Path;
import android.graphics.RectF;
import android.util.SparseBooleanArray;
import android.view.HapticFeedbackConstants;
import android.view.MotionEvent;
import android.view.View;

/**
 * A touch screen's gamepad, over the game: a D-pad at the bottom left, A
 * and B at the bottom right, as a NES pad has them, Select over the D-pad
 * and Start over A and B, in the bars beside the picture. Drawn by Android,
 * at the screen's own resolution, sized in dp, as any app's buttons are.
 * Any number of fingers at once; one that lands on the D-pad keeps it
 * however far it slides, and one sliding between A and B, or resting
 * between them, holds what it's on. What's held is told as it changes, by
 * the bits of the emulator's touch module, each new press a tick of the
 * phone's touch feedback, as its settings have it.
 */
public class TouchPad extends View {
    /** As the emulator's touch module has them. */
    static final int A = 1, B = 1 << 1, SELECT = 1 << 2, START = 1 << 3;
    static final int UP = 1 << 4, DOWN = 1 << 5, LEFT = 1 << 6, RIGHT = 1 << 7;

    /** What's told what's held. */
    interface Held {
        void held(int buttons);
    }

    // Its measures, in dp.
    private static final float MARGIN = 28, DPAD = 70, ARM = 24, BUTTON = 31;
    private static final float BUTTONS_APART = 37, BUTTONS_STAGGER = 20;
    private static final float PILL_WIDTH = 66, PILL_HEIGHT = 26, PILL_ABOVE = 50;
    /** How far past a button's edge a finger still has it, and the D-pad's middle, where it's nothing. */
    private static final float SLOP = 1.3f, DEAD = 0.2f;
    /** How far off an axis a finger on the D-pad still holds that direction alone: 30 degrees. */
    private static final float DIAGONAL = 0.5f;

    private final Held told;
    private final float dp;
    private final Paint fill = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint pressed = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint edge = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint label = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Path cross = new Path();
    private final Path arrow = new Path();
    private final RectF select = new RectF(), start = new RectF(), arm = new RectF();
    /** The fingers that landed on the D-pad, by their pointer ids. */
    private final SparseBooleanArray onDpad = new SparseBooleanArray();

    private float dpadX, dpadY, aX, aY, bX, bY;
    private int held;

    TouchPad(Context context, Held told) {
        super(context);
        this.told = told;
        dp = context.getResources().getDisplayMetrics().density;
        fill.setColor(0x38ffffff);
        pressed.setColor(0x90ffffff);
        edge.setColor(0x99ffffff);
        edge.setStyle(Paint.Style.STROKE);
        edge.setStrokeWidth(2 * dp);
        edge.setStrokeJoin(Paint.Join.ROUND);
        label.setColor(0xccffffff);
        label.setTextAlign(Paint.Align.CENTER);
        label.setFakeBoldText(true);
        setFocusable(false);
        setHapticFeedbackEnabled(true);
    }

    /** All let go, as it's hidden or the game's paused. */
    void letGo() {
        onDpad.clear();
        tell(0);
    }

    @Override
    protected void onSizeChanged(int width, int height, int oldWidth, int oldHeight) {
        float margin = MARGIN * dp, dpad = DPAD * dp;
        dpadX = margin + dpad;
        dpadY = height - margin - dpad;
        float right = width - margin - dpad;
        aX = right + BUTTONS_APART * dp;
        aY = dpadY - BUTTONS_STAGGER * dp;
        bX = right - BUTTONS_APART * dp;
        bY = dpadY + BUTTONS_STAGGER * dp;
        float pillY = dpadY - dpad - PILL_ABOVE * dp;
        float halfWidth = PILL_WIDTH * dp / 2, halfHeight = PILL_HEIGHT * dp / 2;
        select.set(dpadX - halfWidth, pillY - halfHeight, dpadX + halfWidth, pillY + halfHeight);
        start.set(right - halfWidth, pillY - halfHeight, right + halfWidth, pillY + halfHeight);

        // The D-pad's cross: its two bars, as one shape.
        float half = ARM * dp, round = 6 * dp;
        Path across = new Path(), down = new Path();
        across.addRoundRect(dpadX - dpad, dpadY - half, dpadX + dpad, dpadY + half, round, round, Path.Direction.CW);
        down.addRoundRect(dpadX - half, dpadY - dpad, dpadX + half, dpadY + dpad, round, round, Path.Direction.CW);
        cross.reset();
        cross.op(across, down, Path.Op.UNION);
    }

    @Override
    protected void onDraw(Canvas canvas) {
        float dpad = DPAD * dp, half = ARM * dp, round = 6 * dp;
        canvas.drawPath(cross, fill);
        // Each held direction's arm lit, from the middle out.
        if ((held & UP) != 0) {
            arm.set(dpadX - half, dpadY - dpad, dpadX + half, dpadY - half);
            canvas.drawRoundRect(arm, round, round, pressed);
        }
        if ((held & DOWN) != 0) {
            arm.set(dpadX - half, dpadY + half, dpadX + half, dpadY + dpad);
            canvas.drawRoundRect(arm, round, round, pressed);
        }
        if ((held & LEFT) != 0) {
            arm.set(dpadX - dpad, dpadY - half, dpadX - half, dpadY + half);
            canvas.drawRoundRect(arm, round, round, pressed);
        }
        if ((held & RIGHT) != 0) {
            arm.set(dpadX + half, dpadY - half, dpadX + dpad, dpadY + half);
            canvas.drawRoundRect(arm, round, round, pressed);
        }
        canvas.drawPath(cross, edge);
        // An arrow in each arm, pointing out.
        float tip = dpad - 12 * dp, base = dpad - 30 * dp, wide = 10 * dp;
        for (int turn = 0; turn < 4; turn++) {
            canvas.save();
            canvas.rotate(90 * turn, dpadX, dpadY);
            arrow.reset();
            arrow.moveTo(dpadX + tip, dpadY);
            arrow.lineTo(dpadX + base, dpadY - wide);
            arrow.lineTo(dpadX + base, dpadY + wide);
            arrow.close();
            canvas.drawPath(arrow, label);
            canvas.restore();
        }

        button(canvas, aX, aY, "A", (held & A) != 0);
        button(canvas, bX, bY, "B", (held & B) != 0);
        pill(canvas, select, "SELECT", (held & SELECT) != 0);
        pill(canvas, start, "START", (held & START) != 0);
    }

    private void button(Canvas canvas, float x, float y, String name, boolean down) {
        float radius = BUTTON * dp;
        canvas.drawCircle(x, y, radius, down ? pressed : fill);
        canvas.drawCircle(x, y, radius, edge);
        label.setTextSize(22 * dp);
        canvas.drawText(name, x, y - (label.ascent() + label.descent()) / 2, label);
    }

    private void pill(Canvas canvas, RectF where, String name, boolean down) {
        float round = where.height() / 2;
        canvas.drawRoundRect(where, round, round, down ? pressed : fill);
        canvas.drawRoundRect(where, round, round, edge);
        label.setTextSize(11 * dp);
        canvas.drawText(name, where.centerX(), where.centerY() - (label.ascent() + label.descent()) / 2, label);
    }

    @Override
    public boolean onTouchEvent(MotionEvent event) {
        int action = event.getActionMasked();
        if (action == MotionEvent.ACTION_CANCEL) {
            letGo();
            return true;
        }
        // The finger lifted, as this event has it, left out of what's held.
        int lifted = action == MotionEvent.ACTION_UP || action == MotionEvent.ACTION_POINTER_UP
                ? event.getActionIndex() : -1;
        if (action == MotionEvent.ACTION_DOWN || action == MotionEvent.ACTION_POINTER_DOWN) {
            int landed = event.getActionIndex();
            onDpad.put(event.getPointerId(landed),
                    onDpad(event.getX(landed), event.getY(landed)));
        }
        int buttons = 0;
        for (int n = 0; n < event.getPointerCount(); n++) {
            if (n == lifted) {
                continue;
            }
            float x = event.getX(n), y = event.getY(n);
            buttons |= onDpad.get(event.getPointerId(n)) ? directions(x, y) : buttons(x, y);
        }
        if (lifted >= 0) {
            onDpad.delete(event.getPointerId(lifted));
        }
        tell(buttons);
        return true;
    }

    /** Whether a finger landing at `x`, `y` lands on the D-pad. */
    private boolean onDpad(float x, float y) {
        return Math.hypot(x - dpadX, y - dpadY) <= DPAD * dp * SLOP;
    }

    /**
     * The directions a finger on the D-pad holds: its way from the middle,
     * one alone within 30 degrees of it, two between; nothing in the middle.
     */
    private int directions(float x, float y) {
        float dx = x - dpadX, dy = y - dpadY;
        double away = Math.hypot(dx, dy);
        if (away < DPAD * dp * DEAD) {
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
        float reach = BUTTON * dp * SLOP;
        // Between A and B, both.
        if (Math.hypot(x - (aX + bX) / 2, y - (aY + bY) / 2) < BUTTON * dp * 0.6f) {
            return A | B;
        }
        if (Math.hypot(x - aX, y - aY) <= reach) {
            return A;
        }
        if (Math.hypot(x - bX, y - bY) <= reach) {
            return B;
        }
        float slop = 10 * dp;
        if (x >= select.left - slop && x <= select.right + slop
                && y >= select.top - slop && y <= select.bottom + slop) {
            return SELECT;
        }
        if (x >= start.left - slop && x <= start.right + slop
                && y >= start.top - slop && y <= start.bottom + slop) {
            return START;
        }
        return 0;
    }

    private void tell(int buttons) {
        if (buttons == held) {
            return;
        }
        if ((buttons & ~held) != 0) {
            performHapticFeedback(HapticFeedbackConstants.VIRTUAL_KEY);
        }
        held = buttons;
        told.held(buttons);
        invalidate();
    }
}
