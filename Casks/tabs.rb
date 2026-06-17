cask "tabs" do
  version "0.2.3"
  sha256 "1c111503365e83487a2a982ccf68a23557721909e5826c5d85e86eb0d29674c6"

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
