cask "tabs" do
  version "0.2.4"
  sha256 "300c114ed91f716d81a9987d0b5482cbad274e766a20a4a8bafa836f1b9619b7"

  url "https://github.com/sohakolan/Tabs/releases/download/v#{version}/Tabs-arm64.dmg"
  name "Tabs"
  desc "Lightweight, fast window switcher"
  homepage "https://github.com/sohakolan/Tabs"

  depends_on macos: :sonoma
  depends_on arch: :arm64

  app "Tabs.app"

  caveats <<~EOS
    Tabs is not notarized by Apple, so macOS blocks it on first launch.
    Clear the quarantine flag, then open it:
      xattr -dr com.apple.quarantine "/Applications/Tabs.app"
  EOS
end
