cask "tabs" do
  version "0.1.0"
  sha256 "32bac27243de086c56f40991e0480498b7df031608300b12100920438d141b1f"

  url "https://github.com/sohakolan/Tabs/releases/download/v#{version}/Tabs-arm64.dmg"
  name "Tabs"
  desc "Commutateur de fenêtres pour macOS"
  homepage "https://github.com/sohakolan/Tabs"

  depends_on macos: ">= :sonoma"
  depends_on arch: :arm64

  app "Tabs.app"

  caveats <<~EOS
    Tabs n'est pas notarisée par Apple. Si macOS bloque l'app au lancement :
      xattr -dr com.apple.quarantine "/Applications/Tabs.app"

    Pour éviter la quarantaine dès l'installation :
      brew install --cask --no-quarantine tabs
  EOS
end
