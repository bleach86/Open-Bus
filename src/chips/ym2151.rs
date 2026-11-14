//! The Yamaha YM2151 (OPM), as ymfm (Aaron Giles', BSD-3) has it: 8
//! channels of 4 operators, frequency modulation by 8 algorithms, an LFO
//! and noise; two timers, their IRQ; its registers reached by an address
//! then a byte, and its status read.
//!
//! Its clock divided by 64 is its sample rate (55,930 Hz at 3.58 MHz), a
//! step of all of it each sample. An operator's phase steps by its
//! channel's key code and fraction (the LFO's PM and detune 2 added in
//! 64ths of a semitone), detune 1 and its multiple; its output's a sine,
//! looked up as a log attenuation, the envelope's added, made linear. Its
//! envelope attacks, decays to its sustain level, sustains and releases at
//! its rates (with key scaling), a step each third sample; total level and
//! the LFO's AM add to it. Operator 1 feeds back on itself; the others
//! are modulated, or summed, as the algorithm has them; channel 7's
//! operator 4 can be noise. The channels are summed, left and right as
//! each is panned, then through the YM3012 DAC's 10-bit mantissa.
//!
//! Its registers, and what they're for, are its data sheet's; ymfm's notes
//! are on the way. A key goes on or off, and a register written takes
//! effect, at the next sample. Each write to its data keeps it busy 64
//! clocks; a write while it's busy is lost. Timer A counts 1024 less its
//! value, timer B 16 times 256 less its, in 64 clocks; as one runs out,
//! its status bit is set, if its IRQ's enabled, and it starts again; in
//! CSM mode timer A keys on all the channels. Its IRQ is either status bit.

use serde::{Deserialize, Serialize};

/// The status register's bits.
const TIMER_A: u8 = 0x01;
const TIMER_B: u8 = 0x02;
const BUSY: u8 = 0x80;

/// Its clocks to a sample, and those a write keeps it busy.
pub const CLOCKS_A_SAMPLE: u32 = 64;
const BUSY_CLOCKS: u32 = 64;

/// The envelope's states.
const ATTACK: usize = 0;
const DECAY: usize = 1;
const SUSTAIN: usize = 2;
const RELEASE: usize = 3;

/// An attenuation past which an operator's silent.
const QUIET: u32 = 0x380;

/// Each channel's operators in the order they're wired, as offsets of
/// their registers: M1, C1, M2, C2 (key on's bits 3 to 6).
const fn operators(channel: usize) -> [usize; 4] {
    [channel, channel + 16, channel + 8, channel + 24]
}

/// The algorithms, as ymfm encodes them: the inputs of operators 2, 3
/// and 4 (bits 0, 1 to 3, 4 to 6, an index into the outputs so far: 0
/// nothing, 1 to 3 an operator's, 5 1+2, 6 1+3, 7 2+3), and whether
/// operators 1, 2 and 3 are in the sum (bits 7 to 9) beside 4.
const fn algorithm(two: u16, three: u16, four: u16, outs: [u16; 3]) -> u16 {
    two | three << 1 | four << 4 | outs[0] << 7 | outs[1] << 8 | outs[2] << 9
}
const ALGORITHMS: [u16; 8] = [
    algorithm(1, 2, 3, [0, 0, 0]),
    algorithm(0, 5, 3, [0, 0, 0]),
    algorithm(0, 2, 6, [0, 0, 0]),
    algorithm(1, 0, 7, [0, 0, 0]),
    algorithm(1, 0, 3, [0, 1, 0]),
    algorithm(1, 1, 1, [0, 1, 1]),
    algorithm(1, 0, 0, [0, 1, 1]),
    algorithm(0, 0, 0, [1, 1, 1]),
];

/// Detune 2's steps, in 64ths of a semitone: 0, 600, 781 and 950 cents.
const DETUNE_2: [i32; 4] = [
    0,
    (600 * 64 + 50) / 100,
    (781 * 64 + 50) / 100,
    (950 * 64 + 50) / 100,
];

/// |sin| over a quarter of a cycle, as an attenuation, 4.8 log: the
/// chip's, from its die.
#[rustfmt::skip]
const SIN: [u16; 256] = [
    0x859, 0x6C3, 0x607, 0x58B, 0x52E, 0x4E4, 0x4A6, 0x471, 0x443, 0x41A, 0x3F5, 0x3D3, 0x3B5, 0x398, 0x37E, 0x365,
    0x34E, 0x339, 0x324, 0x311, 0x2FF, 0x2ED, 0x2DC, 0x2CD, 0x2BD, 0x2AF, 0x2A0, 0x293, 0x286, 0x279, 0x26D, 0x261,
    0x256, 0x24B, 0x240, 0x236, 0x22C, 0x222, 0x218, 0x20F, 0x206, 0x1FD, 0x1F5, 0x1EC, 0x1E4, 0x1DC, 0x1D4, 0x1CD,
    0x1C5, 0x1BE, 0x1B7, 0x1B0, 0x1A9, 0x1A2, 0x19B, 0x195, 0x18F, 0x188, 0x182, 0x17C, 0x177, 0x171, 0x16B, 0x166,
    0x160, 0x15B, 0x155, 0x150, 0x14B, 0x146, 0x141, 0x13C, 0x137, 0x133, 0x12E, 0x129, 0x125, 0x121, 0x11C, 0x118,
    0x114, 0x10F, 0x10B, 0x107, 0x103, 0x0FF, 0x0FB, 0x0F8, 0x0F4, 0x0F0, 0x0EC, 0x0E9, 0x0E5, 0x0E2, 0x0DE, 0x0DB,
    0x0D7, 0x0D4, 0x0D1, 0x0CD, 0x0CA, 0x0C7, 0x0C4, 0x0C1, 0x0BE, 0x0BB, 0x0B8, 0x0B5, 0x0B2, 0x0AF, 0x0AC, 0x0A9,
    0x0A7, 0x0A4, 0x0A1, 0x09F, 0x09C, 0x099, 0x097, 0x094, 0x092, 0x08F, 0x08D, 0x08A, 0x088, 0x086, 0x083, 0x081,
    0x07F, 0x07D, 0x07A, 0x078, 0x076, 0x074, 0x072, 0x070, 0x06E, 0x06C, 0x06A, 0x068, 0x066, 0x064, 0x062, 0x060,
    0x05E, 0x05C, 0x05B, 0x059, 0x057, 0x055, 0x053, 0x052, 0x050, 0x04E, 0x04D, 0x04B, 0x04A, 0x048, 0x046, 0x045,
    0x043, 0x042, 0x040, 0x03F, 0x03E, 0x03C, 0x03B, 0x039, 0x038, 0x037, 0x035, 0x034, 0x033, 0x031, 0x030, 0x02F,
    0x02E, 0x02D, 0x02B, 0x02A, 0x029, 0x028, 0x027, 0x026, 0x025, 0x024, 0x023, 0x022, 0x021, 0x020, 0x01F, 0x01E,
    0x01D, 0x01C, 0x01B, 0x01A, 0x019, 0x018, 0x017, 0x017, 0x016, 0x015, 0x014, 0x014, 0x013, 0x012, 0x011, 0x011,
    0x010, 0x00F, 0x00F, 0x00E, 0x00D, 0x00D, 0x00C, 0x00C, 0x00B, 0x00A, 0x00A, 0x009, 0x009, 0x008, 0x008, 0x007,
    0x007, 0x007, 0x006, 0x006, 0x005, 0x005, 0x005, 0x004, 0x004, 0x004, 0x003, 0x003, 0x003, 0x002, 0x002, 0x002,
    0x002, 0x001, 0x001, 0x001, 0x001, 0x001, 0x001, 0x001, 0x000, 0x000, 0x000, 0x000, 0x000, 0x000, 0x000, 0x000,
];

/// 2 to the minus a fraction, its 10-bit mantissas with the implied 0x400
/// in, shifted up 2, in reverse: the chip's, from its die.
#[rustfmt::skip]
const POWER: [u16; 256] = [
    0x1FE8, 0x1FD4, 0x1FBC, 0x1FA8, 0x1F90, 0x1F7C, 0x1F68, 0x1F50,
    0x1F3C, 0x1F24, 0x1F10, 0x1EFC, 0x1EE4, 0x1ED0, 0x1EB8, 0x1EA4,
    0x1E90, 0x1E7C, 0x1E64, 0x1E50, 0x1E3C, 0x1E28, 0x1E10, 0x1DFC,
    0x1DE8, 0x1DD4, 0x1DC0, 0x1DA8, 0x1D94, 0x1D80, 0x1D6C, 0x1D58,
    0x1D44, 0x1D30, 0x1D1C, 0x1D08, 0x1CF4, 0x1CE0, 0x1CCC, 0x1CB8,
    0x1CA4, 0x1C90, 0x1C7C, 0x1C68, 0x1C54, 0x1C40, 0x1C2C, 0x1C18,
    0x1C08, 0x1BF4, 0x1BE0, 0x1BCC, 0x1BB8, 0x1BA4, 0x1B94, 0x1B80,
    0x1B6C, 0x1B58, 0x1B48, 0x1B34, 0x1B20, 0x1B10, 0x1AFC, 0x1AE8,
    0x1AD4, 0x1AC4, 0x1AB0, 0x1AA0, 0x1A8C, 0x1A78, 0x1A68, 0x1A54,
    0x1A44, 0x1A30, 0x1A20, 0x1A0C, 0x19FC, 0x19E8, 0x19D8, 0x19C4,
    0x19B4, 0x19A0, 0x1990, 0x197C, 0x196C, 0x195C, 0x1948, 0x1938,
    0x1924, 0x1914, 0x1904, 0x18F0, 0x18E0, 0x18D0, 0x18C0, 0x18AC,
    0x189C, 0x188C, 0x1878, 0x1868, 0x1858, 0x1848, 0x1838, 0x1824,
    0x1814, 0x1804, 0x17F4, 0x17E4, 0x17D4, 0x17C0, 0x17B0, 0x17A0,
    0x1790, 0x1780, 0x1770, 0x1760, 0x1750, 0x1740, 0x1730, 0x1720,
    0x1710, 0x1700, 0x16F0, 0x16E0, 0x16D0, 0x16C0, 0x16B0, 0x16A0,
    0x1690, 0x1680, 0x1670, 0x1664, 0x1654, 0x1644, 0x1634, 0x1624,
    0x1614, 0x1604, 0x15F8, 0x15E8, 0x15D8, 0x15C8, 0x15BC, 0x15AC,
    0x159C, 0x158C, 0x1580, 0x1570, 0x1560, 0x1550, 0x1544, 0x1534,
    0x1524, 0x1518, 0x1508, 0x14F8, 0x14EC, 0x14DC, 0x14D0, 0x14C0,
    0x14B0, 0x14A4, 0x1494, 0x1488, 0x1478, 0x146C, 0x145C, 0x1450,
    0x1440, 0x1430, 0x1424, 0x1418, 0x1408, 0x13FC, 0x13EC, 0x13E0,
    0x13D0, 0x13C4, 0x13B4, 0x13A8, 0x139C, 0x138C, 0x1380, 0x1370,
    0x1364, 0x1358, 0x1348, 0x133C, 0x1330, 0x1320, 0x1314, 0x1308,
    0x12F8, 0x12EC, 0x12E0, 0x12D4, 0x12C4, 0x12B8, 0x12AC, 0x12A0,
    0x1290, 0x1284, 0x1278, 0x126C, 0x1260, 0x1250, 0x1244, 0x1238,
    0x122C, 0x1220, 0x1214, 0x1208, 0x11F8, 0x11EC, 0x11E0, 0x11D4,
    0x11C8, 0x11BC, 0x11B0, 0x11A4, 0x1198, 0x118C, 0x1180, 0x1174,
    0x1168, 0x115C, 0x1150, 0x1144, 0x1138, 0x112C, 0x1120, 0x1114,
    0x1108, 0x10FC, 0x10F0, 0x10E4, 0x10D8, 0x10CC, 0x10C0, 0x10B4,
    0x10A8, 0x10A0, 0x1094, 0x1088, 0x107C, 0x1070, 0x1064, 0x1058,
    0x1050, 0x1044, 0x1038, 0x102C, 0x1020, 0x1018, 0x100C, 0x1000,
];

/// The envelope's increments, by rate, 8 steps of 4 bits.
#[rustfmt::skip]
const INCREMENTS: [u32; 64] = [
    0x00000000, 0x00000000, 0x10101010, 0x10101010,
    0x10101010, 0x10101010, 0x11101110, 0x11101110,
    0x10101010, 0x10111010, 0x11101110, 0x11111110,
    0x10101010, 0x10111010, 0x11101110, 0x11111110,
    0x10101010, 0x10111010, 0x11101110, 0x11111110,
    0x10101010, 0x10111010, 0x11101110, 0x11111110,
    0x10101010, 0x10111010, 0x11101110, 0x11111110,
    0x10101010, 0x10111010, 0x11101110, 0x11111110,
    0x10101010, 0x10111010, 0x11101110, 0x11111110,
    0x10101010, 0x10111010, 0x11101110, 0x11111110,
    0x10101010, 0x10111010, 0x11101110, 0x11111110,
    0x10101010, 0x10111010, 0x11101110, 0x11111110,
    0x11111111, 0x21112111, 0x21212121, 0x22212221,
    0x22222222, 0x42224222, 0x42424242, 0x44424442,
    0x44444444, 0x84448444, 0x84848484, 0x88848884,
    0x88888888, 0x88888888, 0x88888888, 0x88888888,
];

/// Detune 1's steps, by key code, for detunes 0 to 3.
#[rustfmt::skip]
const DETUNE: [[u8; 4]; 32] = [
    [0, 0, 1, 2],
    [0, 0, 1, 2],
    [0, 0, 1, 2],
    [0, 0, 1, 2],
    [0, 1, 2, 2],
    [0, 1, 2, 3],
    [0, 1, 2, 3],
    [0, 1, 2, 3],
    [0, 1, 2, 4],
    [0, 1, 3, 4],
    [0, 1, 3, 4],
    [0, 1, 3, 5],
    [0, 2, 4, 5],
    [0, 2, 4, 6],
    [0, 2, 4, 6],
    [0, 2, 5, 7],
    [0, 2, 5, 8],
    [0, 3, 6, 8],
    [0, 3, 6, 9],
    [0, 3, 7, 10],
    [0, 4, 8, 11],
    [0, 4, 8, 12],
    [0, 4, 9, 13],
    [0, 5, 10, 14],
    [0, 5, 11, 16],
    [0, 6, 12, 17],
    [0, 6, 13, 19],
    [0, 7, 14, 20],
    [0, 8, 16, 22],
    [0, 8, 16, 22],
    [0, 8, 16, 22],
    [0, 8, 16, 22],
];

/// Phase steps by key code and fraction over an octave, the top one's:
/// David Viens' measurements of the chip, as ymfm has them.
#[rustfmt::skip]
const PHASE_STEPS: [u32; 768] = [
    41568, 41600, 41632, 41664, 41696, 41728, 41760, 41792, 41856, 41888, 41920, 41952, 42016, 42048, 42080, 42112,
    42176, 42208, 42240, 42272, 42304, 42336, 42368, 42400, 42464, 42496, 42528, 42560, 42624, 42656, 42688, 42720,
    42784, 42816, 42848, 42880, 42912, 42944, 42976, 43008, 43072, 43104, 43136, 43168, 43232, 43264, 43296, 43328,
    43392, 43424, 43456, 43488, 43552, 43584, 43616, 43648, 43712, 43744, 43776, 43808, 43872, 43904, 43936, 43968,
    44032, 44064, 44096, 44128, 44192, 44224, 44256, 44288, 44352, 44384, 44416, 44448, 44512, 44544, 44576, 44608,
    44672, 44704, 44736, 44768, 44832, 44864, 44896, 44928, 44992, 45024, 45056, 45088, 45152, 45184, 45216, 45248,
    45312, 45344, 45376, 45408, 45472, 45504, 45536, 45568, 45632, 45664, 45728, 45760, 45792, 45824, 45888, 45920,
    45984, 46016, 46048, 46080, 46144, 46176, 46208, 46240, 46304, 46336, 46368, 46400, 46464, 46496, 46528, 46560,
    46656, 46688, 46720, 46752, 46816, 46848, 46880, 46912, 46976, 47008, 47072, 47104, 47136, 47168, 47232, 47264,
    47328, 47360, 47392, 47424, 47488, 47520, 47552, 47584, 47648, 47680, 47744, 47776, 47808, 47840, 47904, 47936,
    48032, 48064, 48096, 48128, 48192, 48224, 48288, 48320, 48384, 48416, 48448, 48480, 48544, 48576, 48640, 48672,
    48736, 48768, 48800, 48832, 48896, 48928, 48992, 49024, 49088, 49120, 49152, 49184, 49248, 49280, 49344, 49376,
    49440, 49472, 49504, 49536, 49600, 49632, 49696, 49728, 49792, 49824, 49856, 49888, 49952, 49984, 50048, 50080,
    50144, 50176, 50208, 50240, 50304, 50336, 50400, 50432, 50496, 50528, 50560, 50592, 50656, 50688, 50752, 50784,
    50880, 50912, 50944, 50976, 51040, 51072, 51136, 51168, 51232, 51264, 51328, 51360, 51424, 51456, 51488, 51520,
    51616, 51648, 51680, 51712, 51776, 51808, 51872, 51904, 51968, 52000, 52064, 52096, 52160, 52192, 52224, 52256,
    52384, 52416, 52448, 52480, 52544, 52576, 52640, 52672, 52736, 52768, 52832, 52864, 52928, 52960, 52992, 53024,
    53120, 53152, 53216, 53248, 53312, 53344, 53408, 53440, 53504, 53536, 53600, 53632, 53696, 53728, 53792, 53824,
    53920, 53952, 54016, 54048, 54112, 54144, 54208, 54240, 54304, 54336, 54400, 54432, 54496, 54528, 54592, 54624,
    54688, 54720, 54784, 54816, 54880, 54912, 54976, 55008, 55072, 55104, 55168, 55200, 55264, 55296, 55360, 55392,
    55488, 55520, 55584, 55616, 55680, 55712, 55776, 55808, 55872, 55936, 55968, 56032, 56064, 56128, 56160, 56224,
    56288, 56320, 56384, 56416, 56480, 56512, 56576, 56608, 56672, 56736, 56768, 56832, 56864, 56928, 56960, 57024,
    57120, 57152, 57216, 57248, 57312, 57376, 57408, 57472, 57536, 57568, 57632, 57664, 57728, 57792, 57824, 57888,
    57952, 57984, 58048, 58080, 58144, 58208, 58240, 58304, 58368, 58400, 58464, 58496, 58560, 58624, 58656, 58720,
    58784, 58816, 58880, 58912, 58976, 59040, 59072, 59136, 59200, 59232, 59296, 59328, 59392, 59456, 59488, 59552,
    59648, 59680, 59744, 59776, 59840, 59904, 59936, 60000, 60064, 60128, 60160, 60224, 60288, 60320, 60384, 60416,
    60512, 60544, 60608, 60640, 60704, 60768, 60800, 60864, 60928, 60992, 61024, 61088, 61152, 61184, 61248, 61280,
    61376, 61408, 61472, 61536, 61600, 61632, 61696, 61760, 61824, 61856, 61920, 61984, 62048, 62080, 62144, 62208,
    62272, 62304, 62368, 62432, 62496, 62528, 62592, 62656, 62720, 62752, 62816, 62880, 62944, 62976, 63040, 63104,
    63200, 63232, 63296, 63360, 63424, 63456, 63520, 63584, 63648, 63680, 63744, 63808, 63872, 63904, 63968, 64032,
    64096, 64128, 64192, 64256, 64320, 64352, 64416, 64480, 64544, 64608, 64672, 64704, 64768, 64832, 64896, 64928,
    65024, 65056, 65120, 65184, 65248, 65312, 65376, 65408, 65504, 65536, 65600, 65664, 65728, 65792, 65856, 65888,
    65984, 66016, 66080, 66144, 66208, 66272, 66336, 66368, 66464, 66496, 66560, 66624, 66688, 66752, 66816, 66848,
    66944, 66976, 67040, 67104, 67168, 67232, 67296, 67328, 67424, 67456, 67520, 67584, 67648, 67712, 67776, 67808,
    67904, 67936, 68000, 68064, 68128, 68192, 68256, 68288, 68384, 68448, 68512, 68544, 68640, 68672, 68736, 68800,
    68896, 68928, 68992, 69056, 69120, 69184, 69248, 69280, 69376, 69440, 69504, 69536, 69632, 69664, 69728, 69792,
    69920, 69952, 70016, 70080, 70144, 70208, 70272, 70304, 70400, 70464, 70528, 70560, 70656, 70688, 70752, 70816,
    70912, 70976, 71040, 71104, 71136, 71232, 71264, 71360, 71424, 71488, 71552, 71616, 71648, 71744, 71776, 71872,
    71968, 72032, 72096, 72160, 72192, 72288, 72320, 72416, 72480, 72544, 72608, 72672, 72704, 72800, 72832, 72928,
    72992, 73056, 73120, 73184, 73216, 73312, 73344, 73440, 73504, 73568, 73632, 73696, 73728, 73824, 73856, 73952,
    74080, 74144, 74208, 74272, 74304, 74400, 74432, 74528, 74592, 74656, 74720, 74784, 74816, 74912, 74944, 75040,
    75136, 75200, 75264, 75328, 75360, 75456, 75488, 75584, 75648, 75712, 75776, 75840, 75872, 75968, 76000, 76096,
    76224, 76288, 76352, 76416, 76448, 76544, 76576, 76672, 76736, 76800, 76864, 76928, 77024, 77120, 77152, 77248,
    77344, 77408, 77472, 77536, 77568, 77664, 77696, 77792, 77856, 77920, 77984, 78048, 78144, 78240, 78272, 78368,
    78464, 78528, 78592, 78656, 78688, 78784, 78816, 78912, 78976, 79040, 79104, 79168, 79264, 79360, 79392, 79488,
    79616, 79680, 79744, 79808, 79840, 79936, 79968, 80064, 80128, 80192, 80256, 80320, 80416, 80512, 80544, 80640,
    80768, 80832, 80896, 80960, 80992, 81088, 81120, 81216, 81280, 81344, 81408, 81472, 81568, 81664, 81696, 81792,
    81952, 82016, 82080, 82144, 82176, 82272, 82304, 82400, 82464, 82528, 82592, 82656, 82752, 82848, 82880, 82976,
];

/// `count` bits of `value` from bit `start`.
fn bits(value: u32, start: u32, count: u32) -> u32 {
    (value >> start) & ((1 << count) - 1)
}

/// |sin| of a 10-bit phase, as a 4.8 log attenuation; its sign in bit 15.
fn sine(phase: u32) -> u32 {
    let quarter = if phase & 0x100 != 0 { !phase } else { phase };
    SIN[(quarter & 0xFF) as usize] as u32 | bits(phase, 9, 1) << 15
}

/// A 5.8 log attenuation as a 13-bit linear volume.
fn volume(attenuation: u32) -> u32 {
    POWER[(attenuation & 0xFF) as usize] as u32 >> (attenuation >> 8)
}

/// The envelope's step at a rate, its `step` of 8.
fn increment(rate: u32, step: u32) -> u32 {
    bits(INCREMENTS[rate as usize], 4 * step, 4)
}

/// Detune 1's step for a 5-bit key code, its sign bit 2 of `detune`.
fn detune(detune: u32, keycode: u32) -> i32 {
    let step = DETUNE[keycode as usize][(detune & 3) as usize] as i32;
    if detune & 4 != 0 { -step } else { step }
}

/// A rate with key scaling: 0 stays 0, the rest at most 63.
fn effective_rate(raw: u32, scaling: u32) -> u32 {
    if raw == 0 { 0 } else { (raw + scaling).min(63) }
}

/// The phase step for a block (3 bits), key code (4) and fraction (6),
/// `delta` 64ths of a semitone added: its key codes 12 to an octave, of
/// 16, the 13th to 16th into the next.
fn phase_step(block_freq: u32, delta: i32) -> u32 {
    let mut block = bits(block_freq, 10, 3);
    let code = bits(block_freq, 6, 4) - bits(block_freq, 8, 2);
    let mut at = ((code << 6) | bits(block_freq, 0, 6)) as i32 + delta;
    if !(0..768).contains(&at) {
        if at < 0 {
            at += 768;
            if block == 0 {
                return PHASE_STEPS[0] >> 7;
            }
            block -= 1;
        } else {
            at -= 768;
            if at >= 768 {
                block += 1;
                at -= 768;
            }
            if block >= 7 {
                return PHASE_STEPS[767];
            }
            block += 1;
        }
    }
    PHASE_STEPS[at as usize] >> (block ^ 7)
}

/// The YM3012's 10.3 floating point, a value through it: its bits below
/// its mantissa's lost.
fn through_dac(value: i32) -> i16 {
    if value < -32768 {
        return -32768;
    }
    if value > 32767 {
        return 32767;
    }
    let scan = (value ^ (value >> 31)) as u32;
    let exponent = (7 - (scan << 17).leading_zeros() as i32).max(1) - 1;
    (value & !((1 << exponent) - 1)) as i16
}

/// What an operator's made of its registers, as they're set.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
struct Cache {
    block_freq: u32,
    detune: i32,
    /// Its multiple, in halves (0 a half).
    multiple: u32,
    /// Its phase step; none with the LFO's PM on it, worked out each step.
    phase_step: Option<u32>,
    total_level: u32,
    sustain: u32,
    rates: [u32; 4],
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct Operator {
    /// Its phase, 10.10.
    phase: u32,
    /// Its envelope's attenuation, 4.6, and its state.
    attenuation: u32,
    state: usize,
    key: bool,
    /// Its key held on: bit 0 by key on, bit 2 by CSM, for a sample.
    keyon: u8,
    cache: Cache,
}

impl Operator {
    fn new() -> Operator {
        Operator {
            phase: 0,
            attenuation: 0x3FF,
            state: RELEASE,
            key: false,
            keyon: 0,
            cache: Cache::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
struct Channel {
    /// Operator 1's last two outputs, and its newest.
    feedback: [i16; 2],
    feedback_in: i16,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ym2151 {
    regs: Vec<u8>,
    address: u8,
    status: u8,
    channels: [Channel; 8],
    operators: [Operator; 32],
    /// The envelope's counter, 2 bits of it a third of a step.
    env_counter: u32,
    samples: u32,
    /// Its registers written since it last looked, and its samples since;
    /// its channels sounding.
    modified: bool,
    prepare_count: u32,
    active: u8,
    /// Its timers' clocks to go, if they're running; its busy clocks; its
    /// clocks into this sample.
    timers: [Option<u32>; 2],
    busy: u32,
    clock: u32,
    lfo_counter: u32,
    noise_lfsr: u32,
    noise_counter: u32,
    noise_state: u32,
    lfo_am: u32,
    /// The LFO's noise waveform, filled in as it goes.
    lfo_noise: Vec<u16>,
    /// Its last sample, left and right.
    output: (i16, i16),
}

impl Default for Ym2151 {
    fn default() -> Ym2151 {
        Ym2151::new()
    }
}

impl Ym2151 {
    /// As it's reset: its registers 0, its operators released.
    pub fn new() -> Ym2151 {
        Ym2151 {
            regs: vec![0; 256],
            address: 0,
            status: 0,
            channels: [Channel::default(); 8],
            operators: [Operator::new(); 32],
            env_counter: 0,
            samples: 0,
            modified: true,
            prepare_count: 0,
            active: 0xFF,
            timers: [None; 2],
            busy: 0,
            clock: 0,
            lfo_counter: 0,
            noise_lfsr: 1,
            noise_counter: 0,
            noise_state: 0,
            lfo_am: 0,
            lfo_noise: vec![0; 256],
            output: (0, 0),
        }
    }

    fn reg(&self, index: usize) -> u32 {
        self.regs[index] as u32
    }

    /// Its status: its timers run out, bits 0 and 1; busy, bit 7.
    pub fn status(&self) -> u8 {
        self.status | if self.busy > 0 { BUSY } else { 0 }
    }

    /// Whether it's holding its IRQ line: a timer's run out, its IRQ on.
    pub fn irq(&self) -> bool {
        self.status & (TIMER_A | TIMER_B) != 0
    }

    /// Its last sample, left and right.
    pub fn output(&self) -> (i16, i16) {
        self.output
    }

    pub fn write_address(&mut self, address: u8) {
        self.address = address;
    }

    /// `data` written to the register at its address, unless it's busy;
    /// busy then.
    pub fn write_data(&mut self, data: u8) {
        if self.busy > 0 {
            return;
        }
        self.write(self.address as usize, data);
        self.busy = BUSY_CLOCKS;
    }

    fn write(&mut self, index: usize, data: u8) {
        if index == 0x14 {
            self.write_mode(data);
            return;
        }
        self.modified = true;
        // The LFO's AM and PM depths share 0x19; PM's kept at 0x1A.
        match index {
            0x19 => self.regs[0x19 + (data >> 7) as usize] = data,
            0x1A => {}
            _ => self.regs[index] = data,
        }
        if index == 0x08 {
            let channel = (data & 7) as usize;
            for (n, op) in operators(channel).into_iter().enumerate() {
                let on = data >> (3 + n) & 1;
                self.operators[op].keyon = self.operators[op].keyon & !1 | on;
            }
        }
    }

    /// The mode register, 0x14: timers' status reset, timers loaded.
    fn write_mode(&mut self, data: u8) {
        self.modified = true;
        self.regs[0x14] = data;
        let mut reset = 0;
        if data & 0x20 != 0 {
            reset |= TIMER_B;
        }
        if data & 0x10 != 0 {
            reset |= TIMER_A;
        }
        self.status &= !reset;
        // Timer B's first count is short, its x16 running free.
        self.update_timer(1, data & 2 != 0, -((self.samples & 15) as i32));
        self.update_timer(0, data & 1 != 0, 0);
    }

    /// Timer `n` started, if it's to run and isn't; stopped, if it's not.
    fn update_timer(&mut self, n: usize, run: bool, delta: i32) {
        if run && self.timers[n].is_none() {
            let period = if n == 0 {
                1024 - (self.reg(0x10) << 2 | self.reg(0x11) & 3)
            } else {
                16 * (256 - self.reg(0x12))
            };
            let period = (period as i32 + delta).max(1) as u32;
            self.timers[n] = Some(period * CLOCKS_A_SAMPLE);
        } else if !run {
            self.timers[n] = None;
        }
    }

    /// Timer `n` run out: its status, if its IRQ's enabled; CSM's key on,
    /// for timer A; started again.
    fn timer_expired(&mut self, n: usize) {
        let mode = self.reg(0x14);
        if n == 0 && mode & 0x04 != 0 {
            self.status |= TIMER_A;
        } else if n == 1 && mode & 0x08 != 0 {
            self.status |= TIMER_B;
        }
        if n == 0 && mode & 0x80 != 0 {
            for op in &mut self.operators {
                op.keyon |= 4;
            }
            self.modified = true;
        }
        self.timers[n] = None;
        self.update_timer(n, true, 0);
    }

    /// A clock of its own: its timers and busy counted, and each 64th, a
    /// sample.
    pub fn tick(&mut self) {
        self.busy = self.busy.saturating_sub(1);
        for n in 0..2 {
            if let Some(left) = self.timers[n] {
                if left <= 1 {
                    self.timer_expired(n);
                } else {
                    self.timers[n] = Some(left - 1);
                }
            }
        }
        self.clock += 1;
        if self.clock == CLOCKS_A_SAMPLE {
            self.clock = 0;
            self.sample();
        }
    }

    /// A sample: everything stepped, the channels summed through the DAC.
    fn sample(&mut self) {
        self.samples = self.samples.wrapping_add(1);
        let due = self.prepare_count >= 4096;
        if self.modified || due {
            self.active = 0;
            for channel in 0..8 {
                if self.prepare(channel) {
                    self.active |= 1 << channel;
                }
            }
            (self.modified, self.prepare_count) = (false, 0);
        } else {
            self.prepare_count += 1;
        }
        // The envelope's stepped each third sample.
        self.env_counter = self.env_counter.wrapping_add(1);
        if self.env_counter & 3 == 3 {
            self.env_counter = self.env_counter.wrapping_add(1);
        }
        let pm = self.noise_and_lfo();
        for channel in 0..8 {
            let ch = &mut self.channels[channel];
            ch.feedback = [ch.feedback[1], ch.feedback_in];
            for op in operators(channel) {
                if self.env_counter & 3 == 0 {
                    self.envelope(op, self.env_counter >> 2);
                }
                let step = match self.operators[op].cache.phase_step {
                    Some(step) => step,
                    None => self.compute_phase_step(channel, op, pm),
                };
                let phase = &mut self.operators[op].phase;
                *phase = phase.wrapping_add(step);
            }
        }
        let mut out = [0i32; 2];
        for channel in 0..8 {
            if self.active >> channel & 1 != 0 {
                self.output_channel(channel, &mut out);
            }
        }
        self.output = (through_dac(out[0]), through_dac(out[1]));
    }

    /// A channel's operators made of its registers, and their keys taken
    /// as they're held: whether any's sounding.
    fn prepare(&mut self, channel: usize) -> bool {
        let mut sounding = false;
        for op in operators(channel) {
            self.operators[op].cache = self.cache(channel, op);
            let key = self.operators[op].keyon != 0;
            self.key(op, key);
            let operator = &mut self.operators[op];
            operator.keyon &= !4;
            sounding |= operator.state != RELEASE || operator.attenuation < QUIET;
        }
        sounding
    }

    fn cache(&self, channel: usize, op: usize) -> Cache {
        let block_freq = (self.reg(0x28 + channel) & 0x7F) << 6 | self.reg(0x30 + channel) >> 2;
        let keycode = bits(block_freq, 8, 5);
        let multiple = (self.reg(0x40 + op) & 15) * 2;
        let sustain = self.reg(0xE0 + op) >> 4;
        let scaling = keycode >> ((self.reg(0x80 + op) >> 6) ^ 3);
        let mut cache = Cache {
            block_freq,
            detune: detune(bits(self.reg(0x40 + op), 4, 3), keycode),
            multiple: multiple.max(1),
            phase_step: None,
            total_level: (self.reg(0x60 + op) & 0x7F) << 3,
            // 15 is 31.
            sustain: (sustain | (sustain + 1) & 0x10) << 5,
            rates: [
                effective_rate((self.reg(0x80 + op) & 31) * 2, scaling),
                effective_rate((self.reg(0xA0 + op) & 31) * 2, scaling),
                effective_rate((self.reg(0xC0 + op) & 31) * 2, scaling),
                effective_rate((self.reg(0xE0 + op) & 15) * 4 + 2, scaling),
            ],
        };
        if self.reg(0x1A) & 0x7F == 0 || bits(self.reg(0x38 + channel), 4, 3) == 0 {
            cache.phase_step = Some(self.phase_step_of(channel, op, &cache, 0));
        }
        cache
    }

    fn compute_phase_step(&self, channel: usize, op: usize, pm: i32) -> u32 {
        self.phase_step_of(channel, op, &self.operators[op].cache, pm)
    }

    /// An operator's phase step: detune 2 and the LFO's PM added to its
    /// key code, detune 1, its multiple.
    fn phase_step_of(&self, channel: usize, op: usize, cache: &Cache, pm: i32) -> u32 {
        let mut delta = DETUNE_2[(self.reg(0xC0 + op) >> 6) as usize];
        let sensitivity = bits(self.reg(0x38 + channel), 4, 3);
        if sensitivity != 0 {
            delta += if sensitivity < 6 {
                pm >> (6 - sensitivity)
            } else {
                ((pm as u32) << (sensitivity - 5)) as i32
            };
        }
        let step = phase_step(cache.block_freq, delta).wrapping_add(cache.detune as u32);
        step.wrapping_mul(cache.multiple) >> 1
    }

    /// An operator's key, on or off: attack, or release, as it changes.
    fn key(&mut self, op: usize, on: bool) {
        let operator = &mut self.operators[op];
        if on == operator.key {
            return;
        }
        operator.key = on;
        if on {
            if operator.state != ATTACK {
                operator.state = ATTACK;
                operator.phase = 0;
                if operator.cache.rates[ATTACK] >= 62 {
                    operator.attenuation = 0;
                }
            }
        } else if operator.state < RELEASE {
            operator.state = RELEASE;
        }
    }

    /// An operator's envelope stepped, `counter` its count of steps.
    fn envelope(&mut self, op: usize, counter: u32) {
        let operator = &mut self.operators[op];
        if operator.state == ATTACK && operator.attenuation == 0 {
            operator.state = DECAY;
        }
        if operator.state == DECAY && operator.attenuation >= operator.cache.sustain {
            operator.state = SUSTAIN;
        }
        let rate = operator.cache.rates[operator.state];
        let shift = rate >> 2;
        let counter = counter << shift;
        if bits(counter, 0, 11) != 0 {
            return;
        }
        let step = increment(rate, bits(counter, shift.max(11), 3));
        if operator.state == ATTACK {
            // Rates 62 and 63 jump at key on, and don't step after.
            if rate < 62 {
                let attenuation = operator.attenuation as i32;
                let change = (-(attenuation + 1) * step as i32) >> 4;
                operator.attenuation = (attenuation + change) as u32;
            }
        } else {
            operator.attenuation = (operator.attenuation + step).min(0x3FF);
        }
    }

    /// The noise and the LFO stepped: the LFO's PM, its AM kept.
    fn noise_and_lfo(&mut self) -> i32 {
        let frequency = (self.reg(0x0F) & 0x1F) ^ 0x1F;
        for _ in 0..2 {
            let lfsr = self.noise_lfsr;
            self.noise_lfsr = lfsr << 1 | (bits(lfsr << 1, 17, 1) ^ bits(lfsr << 1, 14, 1) ^ 1);
            let count = self.noise_counter;
            self.noise_counter += 1;
            if count >= frequency {
                self.noise_counter = 0;
                self.noise_state = bits(self.noise_lfsr, 17, 1);
            }
        }
        let rate = self.reg(0x18);
        self.lfo_counter = self
            .lfo_counter
            .wrapping_add((0x10 | rate & 15) << (rate >> 4));
        if self.reg(0x01) & 2 != 0 {
            self.lfo_counter = 0;
        }
        let lfo = bits(self.lfo_counter, 22, 8);
        let noise = bits(self.noise_lfsr, 17, 8) as u16;
        self.lfo_noise[((lfo + 1) & 0xFF) as usize] = noise | noise << 8;
        let index = lfo as u8;
        let (am, pm): (u8, u8) = match self.reg(0x1B) & 3 {
            0 => (index ^ 0xFF, index),
            1 => {
                let am = if index & 0x80 != 0 { 0 } else { 0xFF };
                (am, am ^ 0x80)
            }
            2 => {
                let am = if index & 0x80 != 0 {
                    index << 1
                } else {
                    (index ^ 0xFF) << 1
                };
                (am, if index & 0x40 != 0 { am } else { !am })
            }
            _ => {
                let [am, pm] = self.lfo_noise[lfo as usize].to_le_bytes();
                (am, pm)
            }
        };
        self.lfo_am = (am as u32 * (self.reg(0x19) & 0x7F)) >> 7;
        (pm as i8 as i32 * (self.reg(0x1A) & 0x7F) as i32) >> 7
    }

    /// The LFO's AM for a channel, as its sensitivity shifts it.
    fn am_offset(&self, channel: usize) -> u32 {
        match self.reg(0x38 + channel) & 3 {
            0 => 0,
            sensitivity => self.lfo_am << (sensitivity - 1),
        }
    }

    /// An operator's envelope as an attenuation: the LFO's AM, if it
    /// takes it, and its total level added.
    fn envelope_attenuation(&self, op: usize, am: u32) -> u32 {
        let operator = &self.operators[op];
        let mut attenuation = operator.attenuation;
        if self.reg(0xA0 + op) & 0x80 != 0 {
            attenuation += am;
        }
        (attenuation + operator.cache.total_level).min(0x3FF)
    }

    /// An operator's output, 14 bits signed, at a phase.
    fn operator_output(&self, op: usize, phase: u32, am: u32) -> i32 {
        if self.operators[op].attenuation > QUIET {
            return 0;
        }
        let sine = sine(phase & 0x3FF);
        let envelope = self.envelope_attenuation(op, am) << 2;
        let level = volume((sine & 0x7FFF) + envelope) as i32;
        if sine & 0x8000 != 0 { -level } else { level }
    }

    /// Channel 7's operator 4 as noise: its envelope, not logarithmic.
    fn noise_output(&self, op: usize, am: u32) -> i32 {
        let level = ((self.envelope_attenuation(op, am) ^ 0x3FF) << 1) as i32;
        if self.noise_state & 1 != 0 {
            -level
        } else {
            level
        }
    }

    /// A channel's operators, by its algorithm, added to `out` as it's
    /// panned.
    fn output_channel(&mut self, channel: usize, out: &mut [i32; 2]) {
        let am = self.am_offset(channel);
        let ops = operators(channel);
        let phase = |op: usize| self.operators[op].phase >> 10;
        let control = self.reg(0x20 + channel);
        let feedback = bits(control, 3, 3);
        let ch = self.channels[channel];
        let modulation = if feedback != 0 {
            (ch.feedback[0] as i32 + ch.feedback[1] as i32) >> (10 - feedback)
        } else {
            0
        };
        let one = self.operator_output(ops[0], phase(ops[0]).wrapping_add(modulation as u32), am);
        self.channels[channel].feedback_in = one as i16;
        if control >> 6 == 0 {
            return;
        }
        let algorithm = ALGORITHMS[(control & 7) as usize] as u32;
        let mut outs = [0i32; 8];
        outs[1] = one;
        let input = |outs: &[i32; 8], at: u32| outs[at as usize] >> 1;
        let modulated = |op: usize, by: i32| phase(op).wrapping_add(by as u32);
        let by = input(&outs, bits(algorithm, 0, 1));
        outs[2] = self.operator_output(ops[1], modulated(ops[1], by), am);
        outs[5] = outs[1] + outs[2];
        let by = input(&outs, bits(algorithm, 1, 3));
        outs[3] = self.operator_output(ops[2], modulated(ops[2], by), am);
        outs[6] = outs[1] + outs[3];
        outs[7] = outs[2] + outs[3];
        let mut result = if self.reg(0x0F) & 0x80 != 0 && channel == 7 {
            self.noise_output(ops[3], am)
        } else {
            let by = input(&outs, bits(algorithm, 4, 3));
            self.operator_output(ops[3], modulated(ops[3], by), am)
        };
        for (n, out) in outs[1..4].iter().enumerate() {
            if bits(algorithm, 7 + n as u32, 1) != 0 {
                result = (result + out).clamp(-32768, 32767);
            }
        }
        if control & 0x40 != 0 {
            out[0] += result;
        }
        if control & 0x80 != 0 {
            out[1] += result;
        }
    }
}
