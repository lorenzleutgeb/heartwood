{
  description = "Radicle";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/release-23.11";

    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs = {
        nixpkgs.follows = "nixpkgs";
        flake-utils.follows = "flake-utils";
      };
    };

    flake-utils.url = "github:numtide/flake-utils";

    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };
  };

  nixConfig = {
    keepOutputs = true;
  };

  outputs = {
    self,
    nixpkgs,
    crane,
    flake-utils,
    advisory-db,
    rust-overlay,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      pname = "Heartwood";
      pkgs = import nixpkgs {
        inherit system;
        overlays = [(import rust-overlay)];
      };

      inherit (pkgs) lib;

      rustToolChain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain;
      craneLib = (crane.mkLib pkgs).overrideToolchain rustToolChain;

      srcFilters = path: type:
      # Allow sql schemas
        (lib.hasSuffix "\.sql" path)
        ||
        # Allow diff files for testing purposes
        (lib.hasSuffix "\.diff" path)
        ||
        # Allow md files for testing purposes
        (lib.hasSuffix "\.md" path)
        ||
        # Allow adoc files
        (lib.hasSuffix "\.adoc" path)
        ||
        # Allow systemd service definitions
        (lib.hasSuffix "\.service" path)
        ||
        # Allow man page build script
        (lib.hasSuffix "build-man-pages\.sh" path)
        ||
        # Default filter from crane (allow .rs files)
        (craneLib.filterCargoSources path type);

      src = lib.cleanSourceWith {
        src = ./.;
        filter = srcFilters;
      };

      # Common arguments can be set here to avoid repeating them later
      commonArgs = {
        inherit pname src cargoArtifacts;
        strictDeps = true;

        buildInputs =
          [
            pkgs.git
            # Add additional build inputs here
          ]
          ++ lib.optionals pkgs.stdenv.isDarwin [
            # Additional darwin specific inputs can be set here
            pkgs.libiconv
            pkgs.darwin.apple_sdk.frameworks.Security
          ];
        preBuild = lib.optionalString (self.shortRev or null != null) ''
          export GIT_HEAD=${self.shortRev}
        '';
      };

      # Build *just* the cargo dependencies, so we can reuse
      # all of that work (e.g. via cachix) when running in CI
      cargoArtifacts =
        craneLib.buildDepsOnly commonArgs;

      craneBuild = name: args:
        craneLib.buildPackage (commonArgs
          // {
            inherit (craneLib.crateNameFromCargoToml {cargoToml = ./. + name + "/Cargo.toml";});
            cargoBuildCommand = "cargo build --release -p ${name}";
            doCheck = false;
          }
          // args);

      # Build the listed .adoc files as man pages to the package.
      buildManPages = pages: {
        nativeBuildInputs = [pkgs.asciidoctor];
        postInstall = ''
          for f in ${lib.escapeShellArgs pages} ; do
            cat=''${f%.adoc}
            cat=''${cat##*.}
            [ -d "$out/share/man/man$cat" ] || mkdir -p "$out/share/man/man$cat"
            scripts/build-man-pages.sh "$out/share/man/man$cat" $f
          done
        '';
        outputs = ["out" "man"];
      };

      # Build the actual crate itself, reusing the dependency
      # artifacts from above.
      radicle = craneLib.buildPackage (commonArgs
        // {
          inherit (craneLib.crateNameFromCargoToml {cargoToml = ./radicle/Cargo.toml;});
          doCheck = false;
        }
        // (buildManPages [
          "git-remote-rad.1.adoc"
          "rad.1.adoc"
          "radicle-node.1.adoc"
          "rad-patch.1.adoc"
          "rad-id.1.adoc"
        ]));
    in {
      # Formatter
      formatter = pkgs.alejandra;

      # Set of checks that are run: `nix flake check`
      checks = {
        # Build the crate as part of `nix flake check` for convenience
        inherit radicle;

        # Run clippy (and deny all warnings) on the crate source,
        # again, reusing the dependency artifacts from above.
        #
        # Note that this is done as a separate derivation so that
        # we can block the CI if there are issues here, but not
        # prevent downstream consumers from building our crate by itself.
        clippy = craneLib.cargoClippy (commonArgs
          // {
            cargoClippyExtraArgs = "--all-targets -- --deny warnings";
          });

        doc = craneLib.cargoDoc commonArgs;

        # Check formatting
        fmt = craneLib.cargoFmt {
          inherit pname;
          inherit src;
        };

        # TODO: audits are failing so skip this check for now
        # Audit dependencies
        # audit = craneLib.cargoAudit {
        #   inherit src advisory-db;
        # };

        # Audit licenses
        deny = craneLib.cargoDeny {
          inherit pname src;
        };

        # Run tests with cargo-nextest
        nextest = craneLib.cargoNextest (commonArgs
          // {
            partitions = 1;
            partitionType = "count";
            nativeBuildInputs = [
              # git is required so the sandbox can access it.
              pkgs.git
              # Ensure that `git-remote-rad` is present for testing.
              self.packages.${system}.radicle-remote-helper
            ];
            # Ensure dev is used since we rely on env variables being
            # set in tests.
            buildPhase = ''
              export CARGO_PROFILE=dev;
            '';
          });
      };

      packages =
        {
          default = radicle;
          radicle-full = pkgs.buildEnv {
            name = "radicle-full";
            paths = with self.packages.${system}; [
              default
              radicle-httpd
            ];
          };
          radicle-node = craneBuild "radicle-node" {
            installPhaseCommand = ''
              mkdir -p $out/lib/systemd/system
              cp $src/systemd/radicle-node.service $out/lib/systemd/system/
            '';
          };
          radicle-httpd = craneBuild "radicle-httpd" ({
              installPhaseCommand = ''
                mkdir -p $out/lib/systemd/system
                cp $src/systemd/radicle-httpd.service $out/lib/systemd/system/
              '';
            }
            // (buildManPages [
              "radicle-httpd.1.adoc"
            ]));
        }
        // (lib.genAttrs ["radicle-cli" "radicle-node"] (name: craneBuild name {}));

      apps =
        {
          default = flake-utils.lib.mkApp {
            drv = radicle;
          };

          radicle-full = flake-utils.lib.mkApp {
            name = "rad";
            drv = self.packages.${system}.radicle-full;
          };

          rad = flake-utils.lib.mkApp {
            name = "rad";
            drv = self.packages.${system}.radicle-cli;
          };

          git-remote-rad = flake-utils.lib.mkApp {
            name = "git-remote-rad";
            drv = self.packages.${system}.radicle-remote-helper;
          };
        }
        // (lib.genAttrs ["radicle-node" "radicle-httpd"] (name:
          flake-utils.lib.mkApp {
            inherit name;
            drv = self.packages.${system}.${name};
          }));

      devShells.default = craneLib.devShell {
        # Extra inputs can be added here; cargo and rustc are provided by default.
        packages = [
          pkgs.cargo-watch
          pkgs.cargo-nextest
          pkgs.ripgrep
          pkgs.rust-analyzer
          pkgs.sqlite
        ];
      };
    });
}
