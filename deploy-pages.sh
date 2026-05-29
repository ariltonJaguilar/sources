#!/bin/zsh
# Gera o source list e faz deploy direto no gh-pages
# Rode da raiz do repositório: ./deploy-pages.sh

set -e

REPO_ROOT="$(pwd)"
DEPLOY_TMP="/tmp/gh-pages-deploy-$$"

echo "→ Gerando source list..."
aidoku build sources/*/package.aix --name "Arilton Sources"

echo "→ Preparando branch gh-pages..."
# Cria um worktree temporário apontando para gh-pages (cria o branch se não existir)
if git ls-remote --exit-code --heads origin gh-pages > /dev/null 2>&1; then
    git worktree add "$DEPLOY_TMP" gh-pages
else
    git worktree add --orphan "$DEPLOY_TMP" gh-pages
fi

echo "→ Copiando arquivos gerados..."
cp -r "$REPO_ROOT/public/." "$DEPLOY_TMP/"

echo "→ Fazendo commit e push..."
cd "$DEPLOY_TMP"
git add -A
git commit -m "chore: deploy source list [skip ci]" || echo "(nada novo para commitar)"
git push origin gh-pages --force

echo "→ Limpando worktree..."
cd "$REPO_ROOT"
git worktree remove --force "$DEPLOY_TMP"

echo ""
echo "✓ Deploy concluído!"
echo "  URL: https://ariltonjaguilar.github.io/sources/index.min.json"
