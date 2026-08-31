{
  lib,
  rustPlatform,
}:

rustPlatform.buildRustPackage {
  pname = "keydrill";
  version = "0.1.0";

  src = lib.cleanSource ./.;

  cargoLock.lockFile = ./Cargo.lock;

  meta = {
    description = "Terminal trainer for keyboard shortcuts, answered by pressing them";
    homepage = "https://github.com/sitolam/keydrill";
    license = lib.licenses.gpl3Plus;
    mainProgram = "keydrill";
    platforms = lib.platforms.unix;
  };
}
