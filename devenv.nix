{ pkgs, ... }:

{

  # https://devenv.sh/packages/
  packages = [
    pkgs.git
    pkgs.sqlite
  ];

  enterShell = ''
    git --version
    rustc --version
    echo "sqlite $(sqlite3 -version)"
  '';

  # https://devenv.sh/languages/
  languages.rust = {
    enable = true;
    version = "stable";
  };

  # https://devenv.sh/scripts/
  # https://devenv.sh/pre-commit-hooks/
  # pre-commit.hooks.shellcheck.enable = true;
  pre-commit.hooks = {
    nixpkgs-fmt.enable = true;
  };

  scripts.cbd.exec = "cargo clean && cargo build";
  scripts.cbr.exec = "cargo clean && cargo build --release";

  processes = {
    sync-backend.exec = "cd examples/ && cargo run -p sync-backend";
  };

}
