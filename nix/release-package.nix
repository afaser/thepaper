{
  pkgs,
  buildInputs,
  runtimeDependencies,
  tools,
}: {version, src}:
pkgs.stdenv.mkDerivation {
  pname = "thepaper";
  inherit version src;
  sourceRoot = ".";

  nativeBuildInputs = [
    pkgs.autoPatchelfHook
    pkgs.makeWrapper
    pkgs.patchelf
  ];

  inherit buildInputs runtimeDependencies;

  dontConfigure = true;
  dontBuild = true;

  installPhase = ''
    runHook preInstall
    mkdir -p $out/bin $out/libexec
    install -m755 thepaper $out/libexec/thepaper
    makeWrapper $out/libexec/thepaper $out/bin/thepaper \
      --prefix PATH : ${pkgs.lib.makeBinPath tools}
    runHook postInstall
  '';

  doInstallCheck = true;
  installCheckPhase = ''
    $out/bin/thepaper --version | grep -F "thepaper ${version}"
  '';

  meta = {
    description = "Zero-copy VA-API/Vulkan live video wallpaper for niri";
    license = pkgs.lib.licenses.gpl3Plus;
    mainProgram = "thepaper";
    platforms = ["x86_64-linux"];
  };
}
