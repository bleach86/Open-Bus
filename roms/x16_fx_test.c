/* The Commander X16's VERA FX test: each helper with fixed inputs, into
   video RAM $00000-$17FFF, the same bytes on any X16 that has it right.
   Its regions: lines, 8-bit and 4-bit, in each octant at 5 slopes; filled
   triangles, 8-bit and 4-bit, two edges crossing; the affine helper,
   wrapping and clipped, 8-bit and 4-bit; the cache filled and written,
   masked, transparent, by nibbles, cycling; the multiplier, accumulating
   and subtracting; the 16-bit hop; and its reads (fill lengths, data) at
   $17800. Interrupts off throughout, the KERNAL's IRQ never touching
   VERA's ports in the middle.

   Its reference is x16emu R49's: on x16emu (its emulator registers say
   "16"), it ends at $FFFF, where x16emu dumps video RAM:
     x16emu -rom rom.bin -prg x16_fx_test.bin -run -dump V
   and src/machines/x16/tests/machine.rs has its regions' CRC-32s. */

#include <stdio.h>

#define VREG(n) (*(volatile unsigned char *)(0x9F20 + (n)))
#define ADDR_L VREG(0)
#define ADDR_M VREG(1)
#define ADDR_H VREG(2)
#define DATA0 VREG(3)
#define DATA1 VREG(4)
#define CTRL VREG(5)
#define DC(n) VREG(9 + (n))

/* ADDRx_H's increment bits. */
#define INC1 0x10
#define INC4 0x30
#define INC160 0xD0
#define INC320 0xE0
#define DECR 0x08
#define NIBBLE_INC 0x04
#define NIBBLE_ADDR 0x02

/* Reads kept, not dropped as a cast to void is. */
volatile unsigned char sink;

static unsigned char obs[512];
static unsigned obs_n;

static void note(unsigned char b) { if (obs_n < sizeof obs) obs[obs_n++] = b; }

static void dcsel(unsigned char d) { CTRL = d << 1; }

/* Port `sel` at `a`, its ADDRx_H's other bits `h`; DCSEL left 0. */
static void at(unsigned char sel, unsigned long a, unsigned char h)
{
	CTRL = sel;
	ADDR_L = (unsigned char)a;
	ADDR_M = (unsigned char)(a >> 8);
	ADDR_H = (unsigned char)((a >> 16) & 1) | h;
}

static void fx_off(void)
{
	dcsel(2);
	DC(0) = 0;
	DC(3) = 0;
	CTRL = 0;
}

static void fill(unsigned long a, unsigned long n, unsigned char v)
{
	at(0, a, INC1);
	while (n--) DATA0 = v;
}

/* --- the line draw helper --------------------------------------------- */

/* Each octant's ADDR1 and ADDR0 increments, 8-bit and 4-bit. */
static const unsigned char oct8[8][2] = {
	{INC1, INC320 | DECR}, {INC320 | DECR, INC1}, {INC320 | DECR, INC1 | DECR}, {INC1 | DECR, INC320 | DECR},
	{INC1 | DECR, INC320}, {INC320, INC1 | DECR}, {INC320, INC1}, {INC1, INC320},
};
static const unsigned char oct4[8][2] = {
	{NIBBLE_INC, INC160 | DECR}, {INC160 | DECR, NIBBLE_INC}, {INC160 | DECR, NIBBLE_INC | DECR}, {NIBBLE_INC | DECR, INC160 | DECR},
	{NIBBLE_INC | DECR, INC160}, {INC160, NIBBLE_INC | DECR}, {INC160, NIBBLE_INC}, {NIBBLE_INC, INC160},
};
/* Slopes 0, 0.3, 0.5, 0.75, 1: low and high. */
static const unsigned char slopes[5][2] = {{0, 0}, {0x9A, 0}, {0, 1}, {0x80, 1}, {0, 2}};

static void lines(unsigned char four)
{
	unsigned char o, s, n, colour = 1;
	unsigned long start = four ? 0x8000UL + 50 * 160 + 80 : 50UL * 320 + 160;
	for (o = 0; o < 8; ++o) {
		for (s = 0; s < 5; ++s) {
			dcsel(2);
			DC(0) = 0x01 | (four ? 0x04 : 0);
			at(0, 0, four ? oct4[o][1] : oct8[o][1]);
			dcsel(3);
			DC(0) = slopes[s][0];
			DC(1) = slopes[s][1];
			at(1, start, (four ? oct4[o][0] : oct8[o][0]) | (four && (o & 1) ? NIBBLE_ADDR : 0));
			for (n = 0; n < 30; ++n) DATA1 = colour;
			colour = colour == (four ? 15 : 250) ? 1 : colour + 1;
		}
	}
	fx_off();
}

/* --- the polygon filler ------------------------------------------------ */

static void polygon(unsigned char four, unsigned long base, unsigned x1, unsigned x2,
	unsigned char l_lo, unsigned char l_hi, unsigned char r_lo, unsigned char r_hi, unsigned char colour)
{
	unsigned char row, fl, fh;
	unsigned len, i;
	dcsel(2);
	DC(0) = 0x02 | (four ? 0x04 : 0);
	at(0, base, four ? INC160 : INC320);
	at(1, 0, four ? NIBBLE_INC : INC1);
	dcsel(3);
	DC(0) = l_lo;
	DC(1) = l_hi;
	DC(2) = r_lo;
	DC(3) = r_hi;
	dcsel(4);
	DC(0) = (unsigned char)x1;
	DC(1) = (unsigned char)(x1 >> 8);
	DC(2) = (unsigned char)x2;
	DC(3) = (unsigned char)(x2 >> 8);
	for (row = 0; row < 40; ++row) {
		sink = DATA1;
		dcsel(5);
		fl = DC(2);
		fh = DC(3);
		note(fl);
		note(fh);
		if ((fh & 0xC0) != 0xC0) {
			if (fl & 0x80)
				len = ((unsigned)(fh >> 1) << 3) | ((fl >> 1) & 7);
			else
				len = (fl >> 1) & (four ? 7 : 15);
			if (len > 320) len = 320;
			for (i = 0; i < len; ++i) DATA1 = colour;
		}
		sink = DATA0;
	}
	fx_off();
}

/* --- the affine helper -------------------------------------------------- */

static void affine(unsigned char four, unsigned char clip, unsigned long dest)
{
	unsigned char row, n;
	unsigned x, y;
	dcsel(2);
	DC(0) = 0x03 | (four ? 0x04 : 0);
	DC(1) = 0xA0 | (clip ? 0x02 : 0);   /* tiles at $14000 */
	DC(2) = 0xA4 | 1;                   /* map at $14800, 8 by 8 */
	dcsel(3);
	DC(0) = 0xC0; DC(1) = 0x01;         /* X increment 0.875 */
	DC(2) = 0xC0; DC(3) = 0x00;         /* Y increment 0.375 */
	at(0, dest, INC1);
	for (row = 0; row < 32; ++row) {
		x = (row * 3 + (clip ? 40 : 0)) & 0x7FF;
		y = (row * 5 + 7) & 0x7FF;
		dcsel(4);
		DC(0) = (unsigned char)x;
		DC(1) = (unsigned char)(x >> 8) | ((row & 1) << 7);
		DC(2) = (unsigned char)y;
		DC(3) = (unsigned char)(y >> 8);
		CTRL = 0;
		for (n = 0; n < 48; ++n) DATA0 = DATA1;
	}
	fx_off();
}

/* --- the cache ------------------------------------------------------------ */

static const unsigned char source[16] = {
	0x11, 0x00, 0x33, 0x44, 0x00, 0x66, 0x77, 0x00, 0x9A, 0x0B, 0xC0, 0x0D, 0xEF, 0x01, 0x20, 0x34,
};

static void cache_fill(unsigned char ctrl, unsigned char mult, unsigned long from, unsigned char h, unsigned char count)
{
	unsigned char n;
	dcsel(2);
	DC(0) = ctrl;
	DC(3) = mult;
	at(0, from, h);
	for (n = 0; n < count; ++n) note(DATA0);
}

static void cache_write(unsigned char ctrl, unsigned long to, const unsigned char *masks, unsigned char count)
{
	unsigned char n;
	dcsel(2);
	DC(0) = ctrl;
	at(1, to, INC4);
	for (n = 0; n < count; ++n) DATA1 = masks[n];
}

static const unsigned char masks[5] = {0x00, 0x0F, 0xF0, 0x33, 0xC3};
static const unsigned char masks4[3] = {0x00, 0x5A, 0xA5};

static void cache(void)
{
	unsigned char n;
	at(0, 0x16400, INC1);
	for (n = 0; n < 16; ++n) DATA0 = source[n];
	fill(0x16500, 0x100, 0xEE);
	/* Filled by reads, written through masks. */
	cache_fill(0x20, 0, 0x16400, INC1, 4);
	cache_write(0x40, 0x16500, masks, 5);
	/* Its zeros left unwritten. */
	cache_fill(0x20, 0, 0x16404, INC1, 4);
	cache_write(0xC0, 0x16520, masks, 2);
	/* 4-bit: nibbles, by nibble indexes, and nibble masks. */
	cache_fill(0x24, 0, 0x16408, NIBBLE_INC, 8);
	cache_write(0x44, 0x16540, masks4, 3);
	cache_fill(0x24, 0x02, 0x16400, NIBBLE_INC, 5);
	cache_write(0xC4, 0x16550, masks4, 3);
	/* Two-byte pairs. */
	cache_fill(0x20, 0x01, 0x16408, INC1, 5);
	cache_write(0x40, 0x165C0, masks, 1);
	cache_fill(0x20, 0x09, 0x1640A, INC1, 3);
	cache_write(0x40, 0x165D0, masks, 1);
	/* Set directly; one-byte cycling, alone and with cache writes. */
	dcsel(6);
	DC(0) = 0xA1; DC(1) = 0xB2; DC(2) = 0xC3; DC(3) = 0xD4;
	dcsel(2);
	DC(3) = 0x04;
	DC(0) = 0x10;
	at(0, 0x16580, INC1);
	for (n = 0; n < 6; ++n) DATA0 = 0x55;
	dcsel(2);
	DC(0) = 0x50;
	at(0, 0x165A0, INC4);
	for (n = 0; n < 3; ++n) DATA0 = 0;
	/* Reads cycling the index in polygon mode, as the docs say. */
	dcsel(2);
	DC(0) = 0x12;
	DC(3) = 0;
	at(0, 0x16400, INC1);
	for (n = 0; n < 3; ++n) note(DATA0);
	dcsel(2);
	DC(0) = 0x50;
	at(0, 0x165E0, INC1);
	DATA0 = 0;
	fx_off();
}

/* --- the multiplier --------------------------------------------------------- */

static const int pairs[6][2] = {{69, 420}, {-3, 1000}, {32767, 32767}, {-32767 - 1, 2}, {-1, -1}, {1234, -567}};

static void put_cache(int a, int b)
{
	dcsel(6);
	DC(0) = (unsigned char)a; DC(1) = (unsigned char)(a >> 8);
	DC(2) = (unsigned char)b; DC(3) = (unsigned char)(b >> 8);
}

static void multiplier(void)
{
	unsigned char p;
	dcsel(2);
	DC(0) = 0x40;
	DC(3) = 0x90;              /* reset, multiplier on */
	at(0, 0x16800, INC4);
	for (p = 0; p < 6; ++p) {
		put_cache(pairs[p][0], pairs[p][1]);
		DATA0 = 0;             /* the product */
		dcsel(2);
		DC(3) = 0x50;          /* accumulate it */
		DATA0 = 0;             /* accumulator and product */
		DC(3) = 0x70;          /* subtracting: accumulate */
		DATA0 = 0;
		dcsel(6);
		sink = DC(1);           /* accumulate, by a read */
		DATA0 = 0;
		sink = DC(0);           /* reset, by a read */
		DATA0 = 0;
		dcsel(2);
		DC(3) = 0x10;
	}
	fx_off();
}

/* --- the 16-bit hop ------------------------------------------------------------- */

static void hop(unsigned long to, unsigned char h)
{
	unsigned char n;
	dcsel(2);
	DC(0) = 0x08;
	at(1, to, h);
	for (n = 1; n <= 12; ++n) DATA1 = n;
	fx_off();
}

int main(void)
{
	unsigned n;
	__asm__("sei");
	fill(0, 0x18000UL, 0);
	lines(0);
	lines(1);
	polygon(0, 0xC000, 100, 100, 0x40, 0x7F, 0x80, 0x01, 0x21);
	polygon(0, 0xC000 + 40UL * 320, 50, 200, 0x00, 0x02, 0x00, 0x7E, 0x42);
	polygon(1, 0x12400, 100, 100, 0x80, 0x7F, 0x40, 0x01, 0x07);
	/* The affine helper's tiles: 4 of 8 by 8, and its 8 by 8 map. */
	at(0, 0x14000, INC1);
	for (n = 0; n < 256; ++n) DATA0 = (unsigned char)(n * 7 + (n >> 6));
	at(0, 0x14800, INC1);
	for (n = 0; n < 64; ++n) DATA0 = (unsigned char)(n & 3);
	affine(0, 0, 0x15000);
	affine(0, 1, 0x15600);
	affine(1, 0, 0x15C00);
	cache();
	multiplier();
	hop(0x17000, INC4);
	hop(0x17042, INC4);
	hop(0x17100, INC320);
	at(0, 0x17800, INC1);
	for (n = 0; n < obs_n; ++n) DATA0 = obs[n];
	CTRL = 0;
	__asm__("cli");
	printf("fxtest done %u\n", obs_n);
	if (*(unsigned char *)0x9FBE == '1' && *(unsigned char *)0x9FBF == '6')
		__asm__("jmp $FFFF");
	for (;;) ;
}
