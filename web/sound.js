// The machine's sound, played as it comes: stereo frames, a left then a
// right, queued, a cushion of them first so that a late frame doesn't run
// it dry, and no more than a tenth of a second behind.
const CUSHION = 0.05 * sampleRate * 2;
const MOST = 0.1 * sampleRate * 2;

class Player extends AudioWorkletProcessor {
    constructor() {
        super();
        this.queue = [];
        this.queued = 0;
        this.at = 0;
        this.playing = false;
        // None, as fast-forward or the menu sends: nothing to queue. Queued,
        // it'd never be done with, and nothing after it played.
        this.port.onmessage = (event) => {
            if (event.data.length > 0 && this.queued < MOST) {
                this.queue.push(event.data);
                this.queued += event.data.length;
            }
        };
    }

    next() {
        const first = this.queue[0];
        const sample = first[this.at++];
        this.queued--;
        if (this.at === first.length) {
            this.queue.shift();
            this.at = 0;
        }
        return sample;
    }

    process(_, outputs) {
        const [left, right] = outputs[0];
        if (!this.playing) {
            this.playing = this.queued >= CUSHION;
        }
        for (let n = 0; n < left.length; n++) {
            if (!this.playing || this.queued < 2) {
                this.playing = false;
                left[n] = right[n] = 0;
            } else {
                left[n] = this.next();
                right[n] = this.next();
            }
        }
        return true;
    }
}

registerProcessor('player', Player);
