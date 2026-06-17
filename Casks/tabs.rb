cask "tabs" do
  version "0.2.2"
  sha256 "1fa504dc1e4fe37254f18e315875a66b3cb4b051c9b8516c91467a20765ebd14"

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
