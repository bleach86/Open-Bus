//! Builds the system ROM with cc65: the BIOS (`rom/bios`) and the system
//! software, the menu and the shell (`rom/system`), linked with the SDK's C
//! runtime (`sdk/lib`) and headers (`sdk/include`), as carts are. The art in
//! `rom/art/*.png` goes through the SDK's converter, as a cart's does.
//!
//! Carts are built with the SDK on their own (`sdk/cart.mk`).

use std::{
    env,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const SYSTEM_ROM_SIZE: usize = 0x4000;

fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let (rom, sdk) = (root.join("rom"), root.join("sdk"));
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    println!("cargo:rerun-if-changed={}", rom.display());
    println!("cargo:rerun-if-changed={}", sdk.display());
    println!("cargo:rerun-if-env-changed=CC65_HOME");
    let cc65 = Toolchain::find(sdk.join("include"), out.clone());

    let objects: Vec<PathBuf> = sources(&sdk.join("lib"))
        .iter()
        .map(|source| cc65.object(source, "lib"))
        .collect();
    let machine_lib = cc65.target_library("machine.lib", &objects);

    let mut objects = Vec::new();
    for part in ["bios", "system"] {
        for source in sources(&rom.join(part)) {
            objects.push(cc65.object(&source, part));
        }
    }
    for image in files(&rom.join("art"), &["png"]) {
        let name = image.file_stem().unwrap().to_string_lossy();
        let source = gfx::read(&image)
            .and_then(|pixels| gfx::asm(&name, &pixels, "RODATA", 8))
            .unwrap_or_else(|error| panic!("{error}"));
        fs::create_dir_all(out.join("art")).unwrap();
        let assembly = out.join("art").join(format!("{name}.s"));
        fs::write(&assembly, source).unwrap();
        objects.push(cc65.object(&assembly, "art"));
    }
    objects.push(machine_lib);
    let system = cc65.link(&rom.join("bios/system.cfg"), "system.bin", &objects);
    assert_eq!(system.len(), SYSTEM_ROM_SIZE, "system ROM size");
    fs::write(out.join("system.bin"), system).unwrap();
}

/// The `.c` and `.s` files in `directory`, sorted.
fn sources(directory: &Path) -> Vec<PathBuf> {
    files(directory, &["c", "s"])
}

/// The files in `directory` with one of `extensions`, sorted.
fn files(directory: &Path, extensions: &[&str]) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("{}: {error}", directory.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().and_then(OsStr::to_str).is_some_and(|e| extensions.contains(&e)))
        .collect();
    files.sort();
    files
}

struct Toolchain {
    bin: PathBuf,
    asminc: PathBuf,
    lib: PathBuf,
    include: PathBuf,
    out: PathBuf,
}

impl Toolchain {
    /// cc65 from `CC65_HOME`, `~/cc65` or the system install.
    fn find(include: PathBuf, out: PathBuf) -> Toolchain {
        let home = env::var_os("CC65_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                let home = PathBuf::from(env::var_os("HOME")?).join("cc65");
                home.join("bin/cc65").is_file().then_some(home)
            })
            .unwrap_or_else(|| PathBuf::from("/usr/share/cc65"));
        assert!(
            home.join("lib/none.lib").is_file(),
            "cc65 not found; set CC65_HOME to the cc65 installation directory"
        );
        let bin = if home.join("bin/cc65").is_file() {
            home.join("bin")
        } else {
            PathBuf::from("/usr/bin")
        };
        Toolchain {
            bin,
            asminc: home.join("asminc"),
            lib: home.join("lib"),
            include,
            out,
        }
    }

    fn run(&self, tool: &str, args: &[&OsStr]) {
        let status = Command::new(self.bin.join(tool))
            .args(args)
            .status()
            .unwrap_or_else(|error| panic!("failed to run {tool}: {error}"));
        assert!(status.success(), "{tool} failed with {status}");
    }

    /// Compile (C) or assemble `source` into `<part>/<stem>.o`. Its own
    /// directory is on the include path.
    fn object(&self, source: &Path, part: &str) -> PathBuf {
        let directory = self.out.join(part);
        fs::create_dir_all(&directory).unwrap();
        let stem = source.file_stem().unwrap().to_string_lossy();
        let object = directory.join(format!("{stem}.o"));
        let mut assembly = source.to_path_buf();
        if source.extension() == Some(OsStr::new("c")) {
            assembly = directory.join(format!("{stem}.s"));
            self.run(
                "cc65",
                &[
                    "-t".as_ref(),
                    "none".as_ref(),
                    "--cpu".as_ref(),
                    "6502".as_ref(),
                    "-Oirs".as_ref(),
                    "-g".as_ref(),
                    "-I".as_ref(),
                    self.include.as_ref(),
                    "-I".as_ref(),
                    source.parent().unwrap().as_ref(),
                    "-o".as_ref(),
                    assembly.as_ref(),
                    source.as_ref(),
                ],
            );
        }
        self.run(
            "ca65",
            &[
                "--cpu".as_ref(),
                "6502".as_ref(),
                "-g".as_ref(),
                "-I".as_ref(),
                self.asminc.as_ref(),
                "-I".as_ref(),
                self.include.as_ref(),
                "-o".as_ref(),
                object.as_ref(),
                assembly.as_ref(),
            ],
        );
        object
    }

    /// cc65's `none.lib` with `objects` added; each replaces the module of
    /// the same name (crt0.o, for one).
    fn target_library(&self, name: &str, objects: &[PathBuf]) -> PathBuf {
        let library = self.out.join(name);
        fs::copy(self.lib.join("none.lib"), &library).unwrap();
        let mut args: Vec<&OsStr> = vec!["a".as_ref(), library.as_ref()];
        args.extend(objects.iter().map(|object| object.as_os_str()));
        self.run("ar65", &args);
        library
    }

    /// Link `inputs` with `config`; returns the image.
    fn link(&self, config: &Path, output: &str, inputs: &[PathBuf]) -> Vec<u8> {
        let image = self.out.join(output);
        // The labels too, for the monitor (monitor.rs).
        let labels = image.with_extension("lbl");
        let mut args: Vec<&OsStr> =
            vec!["-C".as_ref(), config.as_ref(), "-Ln".as_ref(), labels.as_ref(), "-o".as_ref(), image.as_ref()];
        args.extend(inputs.iter().map(|input| input.as_os_str()));
        self.run("ld65", &args);
        fs::read(&image).unwrap()
    }
}
