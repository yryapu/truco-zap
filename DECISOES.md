# Decisões de desenho

Elas moram no repositório de pesquisa, por exigência do enunciado:
**https://github.com/yryapu/poliorketikos-truco-zap/tree/main/decisoes**

Este arquivo existe porque eu discordo em parte dessa separação: um ADR longe do código que
ele governa apodrece em silêncio. Então fica aqui o índice, para que quem lê o código ache o
porquê sem adivinhar onde procurar.

| | decisão | resumo |
|---|---|---|
| D01 | axum + tokio | HTTP e WS no mesmo processo; descartado actix (modelo de atores que eu não preciso) |
| D02 | SQLite via sqlx | ACID para o saldo sem um segundo container; descartado Postgres |
| D03 | token opaco em cookie HttpOnly | revogável na hora; descartado JWT |
| D04 | a carta é o code point Unicode | alfabeto de 40 símbolos válidos; descartado `{rank,suit}` |
| D05 | mesa viva em memória, resultado no banco | descartado ator por mesa e event sourcing |
| D06 | **sem WASM no cliente** | 4 motivos e o critério para mudar de ideia |
| D07 | após empate puxa quem jogou a 1ª carta que empatou | divergência real entre as fontes |
| D08 | só baralho sujo (40) | as fontes nem concordam no tamanho do limpo |
| D09 | mão de ferro aberta | a fechada é uma tela sem decisão |
| D10 | truco na mão de onze é comando recusado | não puno um clique que a interface permitiu |
| D11 | 1x1 é extrapolação declarada | nenhuma fonte descreve truco paulista a dois |
| D12 | webhook: fila no banco + HMAC | at-least-once; descartada entrega in-line e Redis |
| D13 | Playwright dentro do Docker | descartados Selenium e teste só de unidade |
| D14 | um subagente, adversarial, sobre o motor | descartado fan-out antes do contrato existir |
| D15 | robôs completam a mesa em 8s | tensão com "nada além disso", resolvida e registrada |
