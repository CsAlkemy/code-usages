# Homebrew cask — lives in YOUR tap repo, not here. Setup (one time):
#
#   1. Create a GitHub repo named  homebrew-tap  under your account.
#   2. Copy this file to  Casks/claude-usage-bar.rb  in that repo.
#   3. After each release, update `version` and `sha256`:
#        shasum -a 256 claude-usage-bar-<version>-universal.dmg
#
# Users then install with:
#   brew install CsAlkemy/tap/claude-usage-bar
cask "claude-usage-bar" do
  version "0.2.0"
  sha256 "TODO_SHA256_OF_DMG"

  url "https://github.com/csalkemy/code-usages/releases/download/v#{version}/claude-usage-bar-#{version}-universal.dmg"
  name "Claude Usage Bar"
  desc "Menu-bar ring showing your claude.ai plan usage"
  homepage "https://github.com/csalkemy/code-usages"

  auto_updates true

  app "Claude Usage Bar.app"

  zap trash: [
    "~/Library/Application Support/Claude Usage Bar",
    "~/Library/Preferences/com.tuliptech.claude-usage-bar.plist",
  ]
end
