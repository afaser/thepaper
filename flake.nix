{
  description = "thepaper: zero-copy VA-API/Vulkan live wallpaper for niri";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = {
    self,
    nixpkgs,
  }: let
    system = "x86_64-linux";
    pkgs = nixpkgs.legacyPackages.${system};
    lib = pkgs.lib;
    cargoToml = lib.importTOML ./Cargo.toml;
    version = cargoToml.package.version;
    ffmpeg = pkgs.ffmpeg;
    tools = [ffmpeg pkgs.libva-utils];
    buildInputs = [
      ffmpeg
      pkgs.libva
      pkgs.libxkbcommon
      pkgs.libglvnd
      pkgs.stdenv.cc.cc.lib
      pkgs.vulkan-loader
      pkgs.libdrm
      pkgs.wayland
    ];
    runtimeDependencies = [
      pkgs.libglvnd
      pkgs.libxkbcommon
      pkgs.vulkan-loader
    ];
    mkReleasePackage = import ./nix/release-package.nix {
      inherit pkgs buildInputs runtimeDependencies tools;
    };
    release =
      if builtins.pathExists ./release.json
      then builtins.fromJSON (builtins.readFile ./release.json)
      else null;
    source = pkgs.rustPlatform.buildRustPackage {
      pname = "thepaper";
      inherit version;
      src = self;
      cargoLock.lockFile = ./Cargo.lock;
      nativeBuildInputs = [
        pkgs.autoPatchelfHook
        pkgs.makeWrapper
        pkgs.patchelf
        pkgs.pkg-config
        pkgs.rustPlatform.bindgenHook
      ];
      inherit buildInputs runtimeDependencies;
      installPhase = ''
        runHook preInstall
        mkdir -p $out/bin $out/libexec
        install -m755 target/x86_64-unknown-linux-gnu/release/thepaper $out/libexec/thepaper
        makeWrapper $out/libexec/thepaper $out/bin/thepaper \
          --prefix PATH : ${lib.makeBinPath tools}
        runHook postInstall
      '';
      doInstallCheck = true;
      installCheckPhase = ''
        $out/bin/thepaper --version | grep -F "thepaper ${version}"
      '';
      meta = {
        description = cargoToml.package.description;
        license = lib.licenses.gpl3Plus;
        mainProgram = "thepaper";
        platforms = ["x86_64-linux"];
      };
    };
    packages = rec {
      inherit source;
      default =
        if release == null
        then source
        else mkReleasePackage {
          inherit (release) version;
          src = pkgs.fetchurl {
            inherit (release) url;
            hash = release.hash;
          };
        };
      thepaper = default;
    };
  in {
    packages.${system} = packages;

    checks.${system} = {
      inherit (packages) source default;
    };

    apps.${system}.default = {
      type = "app";
      program = "${packages.default}/bin/thepaper";
      meta = {
        description = "Zero-copy VA-API/Vulkan live wallpaper for niri";
      };
    };

    lib.mkReleasePackage = mkReleasePackage;

    devShells.${system}.default = pkgs.mkShell {
      packages = [
        pkgs.rustc
        pkgs.cargo
        pkgs.rustfmt
        pkgs.clippy
        pkgs.pkg-config
        pkgs.clang
        pkgs.llvmPackages.libclang
        ffmpeg
        pkgs.libva
        pkgs.libva-utils
        pkgs.vulkan-loader
        pkgs.vulkan-headers
        pkgs.wayland
        pkgs.wayland-protocols
        pkgs.libdrm
        pkgs.mesa
        pkgs.patchelf
      ];

      LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";

      shellHook = ''
        # Vulkan loader must find the Mesa ICD built for this GPU.
        if [ -z "''${VK_ICD_FILENAMES:-}" ] && [ -z "''${VK_DRIVER_FILES:-}" ]; then
          export VK_ICD_FILENAMES="${pkgs.mesa}/share/vulkan/icd.d/radeon_icd.x86_64.json:${pkgs.mesa}/share/vulkan/icd.d/lvp_icd.x86_64.json"
        fi
      '';
    };
  };
}
