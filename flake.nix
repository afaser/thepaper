{
  description = "thepaper: zero-copy VA-API/Vulkan live wallpaper for niri";

  # Pinned to the nixpkgs source registered on this workstation. This keeps the
  # dev shell reproducible without network access; refresh the path (or switch
  # to a github: pin) when nixpkgs is updated.
  inputs.nixpkgs.url = "path:/nix/store/ap9dpkyzikzzh04259wlsvha2mw455x4-source";

  outputs = { self, nixpkgs }: let
    system = "x86_64-linux";
    pkgs = nixpkgs.legacyPackages.${system};
    lib = pkgs.lib;
  in {
    devShells.${system}.default = pkgs.mkShell {
      packages = with pkgs; [
        rustc
        cargo
        rustfmt
        clippy
        pkg-config
        clang
        llvmPackages.libclang
        ffmpeg
        libva
        libva-utils
        vulkan-loader
        vulkan-headers
        wayland
        wayland-protocols
        libdrm
        mesa
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
