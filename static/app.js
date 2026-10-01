// truco-zap — cliente. JavaScript puro, sem build step (decisão D06).
//
// Nenhuma regra de truco vive aqui. O cliente desenha o que o servidor manda e devolve a
// intenção do jogador. Os botões ligam e desligam pela lista `opcoes` que vem no estado —
// é por isso que o botão de truco simplesmente não existe na mão de onze (D10).
'use strict';

const $ = (id) => document.getElementById(id);
const COSTAS = '\u{1F0A0}';
const BASE_NAIPE = { 0x1f0a0: 'espadas', 0x1f0b0: 'copas', 0x1f0c0: 'ouros', 0x1f0d0: 'paus' };
const RANK_POR_OFFSET = { 1: 'A', 2: '2', 3: '3', 4: '4', 5: '5', 6: '6', 7: '7', 0xb: 'J', 0xd: 'Q', 0xe: 'K' };
const ORDEM_RANKS = ['4', '5', '6', '7', 'Q', 'J', 'K', 'A', '2', '3'];

let ws = null;
let modo = '1x1';
let estado = null;
let meuPerfil = null;
let ultimoPedido = null;

const naipeDe = (ch) => BASE_NAIPE[ch.codePointAt(0) & 0xfffffff0] || null;
const rankDe = (ch) => RANK_POR_OFFSET[ch.codePointAt(0) & 0xf] || '?';

/** R3: a manilha é o rank imediatamente acima da vira, em ordem cíclica. Aqui só para rotular. */
function rankDaManilha(vira) {
  const i = ORDEM_RANKS.indexOf(rankDe(vira));
  return i < 0 ? '?' : ORDEM_RANKS[(i + 1) % 10];
}

function elCarta(ch, { manilha = false, classe = '' } = {}) {
  const e = document.createElement('span');
  e.className = 'carta' + (classe ? ' ' + classe : '');
  e.textContent = ch;
  if (ch === COSTAS) {
    e.classList.add('costas');
    e.title = 'carta jogada de costas — não vale nada na rodada';
    e.setAttribute('aria-label', 'carta de costas');
    return e;
  }
  const n = naipeDe(ch);
  if (n === 'copas' || n === 'ouros') e.classList.add('vermelha');
  if (manilha) e.classList.add('manilha');
  e.title = `${rankDe(ch)} de ${n}${manilha ? ' — manilha' : ''}`;
  e.setAttribute('aria-label', e.title);
  return e;
}

function ehManilha(ch, rankManilha) {
  return ch !== COSTAS && rankDe(ch) === rankManilha;
}

async function api(rota, opcoes = {}) {
  const r = await fetch(rota, {
    credentials: 'same-origin',
    headers: { 'content-type': 'application/json' },
    ...opcoes,
  });
  const corpo = await r.json().catch(() => ({}));
  if (!r.ok) throw new Error(corpo.erro || `http_${r.status}`);
  return corpo;
}

// ───────────────────────────────── telas

function mostrar(tela) {
  for (const t of ['tela-entrada', 'tela-lobby', 'tela-mesa']) $(t).hidden = t !== tela;
}

function pintarPerfil(j) {
  meuPerfil = j;
  $('barra-perfil').hidden = false;
  $('perfil-apelido').textContent = j.apelido;
  $('perfil-saldo').textContent = j.saldo;
  $('perfil-placar').textContent = `${j.vitorias}V / ${j.derrotas}D`;
  const alvo = $('emblemas');
  alvo.textContent = '';
  for (const e of j.emblemas) {
    const d = document.createElement('div');
    d.className = 'emblema';
    d.dataset.chave = e.chave;
    const ic = document.createElement('span');
    ic.className = 'ic';
    ic.textContent = e.icone;
    const nm = document.createElement('span');
    nm.textContent = e.nome;
    const pq = document.createElement('span');
    pq.className = 'pq';
    pq.textContent = e.motivo;
    d.append(ic, nm, pq);
    alvo.appendChild(d);
  }
}

function linha(celulas, classe) {
  const tr = document.createElement('tr');
  if (classe) tr.className = classe;
  for (const c of celulas) {
    const td = document.createElement('td');
    if (c && typeof c === 'object') {
      td.className = c.classe || '';
      td.textContent = c.texto;
    } else {
      td.textContent = c;
    }
    tr.appendChild(td);
  }
  return tr;
}

async function carregarLobby() {
  mostrar('tela-lobby');
  try {
    const { ranking } = await api('/api/ranking');
    const t = $('ranking');
    t.textContent = '';
    for (const l of ranking) {
      t.appendChild(
        linha(
          [
            { texto: l.posicao, classe: 'pos' },
            l.apelido,
            { texto: `${l.vitorias}V / ${l.derrotas}D`, classe: 'num' },
            { texto: `${l.saldo}`, classe: 'num moedas-col' },
          ],
          l.apelido === meuPerfil?.apelido ? 'eu-mesmo' : ''
        )
      );
    }
    if (!ranking.length) {
      t.appendChild(linha([{ texto: 'Ninguém pontuou ainda. Ganhe uma e assuma o topo.' }], 'vazio'));
    }
  } catch (_) {}

  try {
    const { historico } = await api('/api/historico');
    const t = $('historico');
    t.textContent = '';
    for (const h of historico) {
      t.appendChild(
        linha([
          h.modo,
          { texto: `${h.placar_time0} × ${h.placar_time1}`, classe: 'num' },
          {
            texto: h.venceu ? `ganhou ${h.premio}` : `perdeu ${h.aposta}`,
            classe: 'num ' + (h.venceu ? 'ganhou' : 'perdeu'),
          },
        ])
      );
    }
    if (!historico.length) {
      t.appendChild(linha([{ texto: 'Nenhuma mesa terminada ainda.' }], 'vazio'));
    }
  } catch (_) {}
  carregarWebhooks();
}

async function carregarWebhooks() {
  try {
    const { webhooks } = await api('/api/webhooks');
    const ul = $('webhooks');
    ul.textContent = '';
    for (const w of webhooks) {
      const li = document.createElement('li');
      li.dataset.id = w.id;
      const url = document.createElement('span');
      url.className = 'url';
      url.textContent = w.url;
      const b = document.createElement('button');
      b.textContent = 'Remover';
      b.className = 'liso';
      b.onclick = async () => {
        await api(`/api/webhooks/${w.id}`, { method: 'DELETE' });
        carregarWebhooks();
      };
      li.append(url, b);
      ul.appendChild(li);
    }
  } catch (_) {}
}

// ───────────────────────────────── WebSocket

function conectar() {
  const esquema = location.protocol === 'https:' ? 'wss' : 'ws';
  ws = new WebSocket(`${esquema}://${location.host}/ws`);
  ws.onmessage = (ev) => receber(JSON.parse(ev.data));
  ws.onclose = () =>
    fala('A conexão caiu. Recarregue a página — a partida continua de onde parou.');
  return new Promise((ok, falha) => {
    ws.onopen = ok;
    ws.onerror = falha;
  });
}

function manda(obj) {
  if (ws && ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify(obj));
}

/** Uma carta citada no meio de uma frase. Em tamanho de corpo o glifo some; aqui ele cresce. */
function cartaInline(ch) {
  const e = document.createElement('span');
  e.className = 'carta-inline' + (['copas', 'ouros'].includes(naipeDe(ch)) ? ' vermelha' : '');
  e.textContent = ch;
  e.title = ch === COSTAS ? 'de costas' : `${rankDe(ch)} de ${naipeDe(ch)}`;
  return e;
}

/** `partes` aceita texto e nós. O primeiro item pode ser um destaque em negrito. */
function fala(partes, destaque) {
  const li = document.createElement('li');
  if (destaque) {
    const b = document.createElement('b');
    b.textContent = destaque;
    li.append(b, document.createTextNode(' '));
  }
  for (const x of [].concat(partes)) {
    li.append(typeof x === 'string' ? document.createTextNode(x) : x);
  }
  $('log').prepend(li);
}

function gritar(txt) {
  const el = $('grito-overlay');
  $('grito-texto').textContent = txt;
  el.classList.remove('bate');
  void el.offsetWidth;
  el.classList.add('bate');
}

const NOME_ERRO = {
  nao_eh_sua_vez: 'Espere a sua vez.',
  carta_nao_esta_na_sua_mao: 'Essa carta não está na sua mão.',
  encoberta_na_primeira_rodada: 'Na primeira rodada a carta vai aberta.',
  truco_proibido_mao_de_onze: 'Na mão de onze não se pede truco.',
  seu_time_fez_o_ultimo_pedido: 'Quem sobe agora é o outro lado.',
  aumento_desnecessario: 'Aceitar já ganha a partida. Não precisa subir.',
  valor_maximo_atingido: 'A mão já vale 12.',
  aposta_invalida: 'Essa aposta passa do seu saldo.',
  saldo_insuficiente: 'Seu saldo não cobre essa aposta.',
  ja_esta_em_partida: 'Você já está numa mesa.',
  carta_desconhecida: 'Essa carta não existe no baralho do truco.',
  assento_inexistente: 'Lugar inválido na mesa.',
};

function receber(m) {
  switch (m.t) {
    case 'ola':
      pintarPerfil(m.jogador);
      break;
    case 'fila':
      $('status-fila').textContent =
        m.faltam > 0
          ? `Procurando ${m.faltam === 1 ? 'mais um jogador' : `mais ${m.faltam} jogadores`} ` +
            `para ${m.modo} valendo ${m.aposta}. Se ninguém aparecer em ${m.robos_em_segundos}s, ` +
            `a mesa completa com robôs.`
          : 'Montando a mesa.';
      break;
    case 'fila_saiu':
      $('status-fila').textContent = 'Você saiu da fila.';
      break;
    case 'mesa':
      $('caixa-fim').hidden = true;
      $('log').textContent = '';
      ultimoPedido = null;
      $('bolo').textContent = m.bolo;
      $('modo-mesa').textContent = `mesa ${m.modo}, aposta de ${m.aposta}`;
      mostrar('tela-mesa');
      fala(`${m.modo}, bolo de ${m.bolo} moedas.`, 'Mesa aberta.');
      break;
    case 'estado':
      estado = m;
      pintarMesa(m);
      break;
    case 'evento':
      narrar(m.evento);
      break;
    case 'erro': {
      const txt = NOME_ERRO[m.codigo] || m.codigo;
      $('erro-mesa').textContent = txt;
      $('status-fila').textContent = txt;
      setTimeout(() => ($('erro-mesa').textContent = ''), 3500);
      break;
    }
    case 'fim':
      $('caixa-fim').hidden = false;
      $('fim').textContent = m.venceu ? 'Vitória!' : 'Derrota.';
      $('fim-detalhe').textContent = m.venceu
        ? `${m.placar[0]} × ${m.placar[1]} — você levou ${m.premio} moedas do bolo.`
        : `${m.placar[0]} × ${m.placar[1]} — fica para a próxima.`;
      if (m.saldo != null && meuPerfil) {
        meuPerfil.saldo = m.saldo;
        $('perfil-saldo').textContent = m.saldo;
      }
      break;
    case 'derrubado':
      fala('Outra aba assumiu esta conta.');
      break;
  }
}

const SELO_TIPO = {
  mao_de_onze: 'Mão de onze — vale 3, sem truco',
  mao_de_ferro: 'Mão de ferro — quem levar, leva a partida',
};
const NOME_PEDIDO = { 3: 'TRUCO!', 6: 'SEIS!', 9: 'NOVE!', 12: 'DOZE!' };

function tipoChave(t) {
  if (typeof t === 'string') return t;
  if (t && typeof t === 'object') return Object.keys(t)[0];
  return 'normal';
}

function narrar(e) {
  const quem = (a) => estado?.jogadores?.[a]?.apelido ?? `assento ${a}`;
  const nosso = (t) => estado && t === estado.time;
  // Concordância: "nós levamos", "eles levaram". Dizer "eles levou" soa a rascunho.
  const levou = (t) => (nosso(t) ? 'Nós levamos' : 'Eles levaram');
  const correu = (t) => (nosso(t) ? 'Nós corremos' : 'Eles correram');
  const posse = (t) => (nosso(t) ? 'nossa' : 'deles');
  switch (e.evento) {
    case 'mao_comecou':
      fala(
        ['vira ', cartaInline(e.vira), `, então a manilha é o ${rankDaManilha(e.vira)}. ` +
         `Começa valendo ${e.valor}.`],
        `Mão ${e.mao}:`
      );
      break;
    case 'carta_jogada':
      fala([`${quem(e.assento)} jogou `, cartaInline(e.carta), e.encoberta ? ', de costas.' : '.']);
      break;
    case 'rodada_terminou':
      fala(
        e.vencedor === null
          ? `A ${e.indice + 1}ª rodada empatou.`
          : `${levou(e.vencedor)} a ${e.indice + 1}ª rodada.`
      );
      break;
    case 'pedido':
      ultimoPedido = e.proposto;
      gritar(NOME_PEDIDO[e.proposto] || `${e.proposto}!`);
      fala(`${quem(e.assento)} pediu ${NOME_PEDIDO[e.proposto] || e.proposto}`, '📣');
      break;
    case 'pedido_aceito':
      fala(`Aceitaram. A mão agora vale ${e.valor}.`);
      break;
    case 'correu':
      fala(`${correu(e.time_que_correu)} — ${e.pontos} ponto(s) para o outro lado.`);
      break;
    case 'mao_de_onze_recusada':
      fala(`Mão de onze recusada. 1 ponto ${nosso(e.pontos_para) ? 'para nós' : 'para eles'}.`);
      break;
    case 'mao_terminou':
      fala(
        e.vencedor === null
          ? 'Mão empatada. Ninguém pontua.'
          : `A mão foi ${posse(e.vencedor)}: +${e.pontos}. Placar ${e.placar[0]} × ${e.placar[1]}.`,
        e.vencedor === null ? '=' : '●'
      );
      break;
    case 'partida_terminou':
      fala(
        `${nosso(e.vencedor) ? 'Nós vencemos' : 'Eles venceram'} por ` +
        `${e.placar[0]} × ${e.placar[1]}.`,
        'Fim.'
      );
      break;
  }
}

function pintarSementes(el, pontos) {
  el.textContent = '';
  for (let i = 0; i < 12; i++) {
    const s = document.createElement('i');
    if (i < Math.min(pontos, 12)) s.className = 'cheia';
    el.appendChild(s);
  }
}

/** Uma carta na mesa, com quem jogou e se levou a rodada. */
function elNaMesa(jogada, { rankManilha, nomes, vencedores, houveVencedor, pequena }) {
  const w = document.createElement('div');
  w.className = 'na-mesa';
  const levou = vencedores.includes(jogada.assento);
  if (houveVencedor && levou) w.classList.add('levou');
  if (houveVencedor && !levou) w.classList.add('perdeu-rodada');

  w.appendChild(elCarta(jogada.carta, {
    manilha: ehManilha(jogada.carta, rankManilha),
    classe: pequena ? 'mini' : '',
  }));

  const nm = document.createElement('span');
  nm.className = 'de-quem';
  nm.textContent = nomes[jogada.assento] ?? '';
  w.appendChild(nm);

  if (houveVencedor && levou) {
    const tag = document.createElement('span');
    tag.className = 'levou-tag';
    tag.textContent = 'levou';
    w.appendChild(tag);
  }
  return w;
}

function pintarMesa(s) {
  const meuTime = s.time;
  const nomes = s.jogadores.map((j) => j.apelido);
  const rankManilha = rankDaManilha(s.vira);

  $('placar-nos').textContent = s.placar[meuTime];
  $('placar-eles').textContent = s.placar[1 - meuTime];
  pintarSementes($('sementes-nos'), s.placar[meuTime]);
  pintarSementes($('sementes-eles'), s.placar[1 - meuTime]);

  $('valor-mao').textContent = s.valor;
  $('manilha').textContent = rankManilha;
  $('modo-mesa').textContent = `mesa ${s.modo}, aposta de ${s.aposta}`;

  $('vira').textContent = '';
  $('vira').appendChild(elCarta(s.vira, { classe: 'mini' }));

  const selo = SELO_TIPO[tipoChave(s.tipo_mao)];
  $('tipo-mao').textContent = selo || '';
  $('tipo-mao').hidden = !selo;

  // jogadores
  const js = $('jogadores');
  js.textContent = '';
  for (const j of s.jogadores) {
    const d = document.createElement('div');
    d.className = 'j' + (j.time === meuTime ? ' nosso' : ' eles');
    d.dataset.assento = j.assento;
    if (j.assento === s.assento) d.classList.add('eu');
    if (s.vez === j.assento) d.classList.add('vez');
    if (!j.online) d.classList.add('off');

    const nome = document.createElement('span');
    nome.className = 'nome';
    nome.textContent = j.apelido + (j.robo ? ' 🤖' : '');
    const lado = document.createElement('span');
    lado.className = 'lado-j';
    lado.textContent = j.time === meuTime ? 'nós' : 'eles';
    const dorsos = document.createElement('span');
    dorsos.className = 'dorsos';
    dorsos.textContent = `${COSTAS}${j.cartas_na_mao}`;
    dorsos.title = `${j.cartas_na_mao} carta(s) na mão`;
    d.append(nome, lado, dorsos);
    js.appendChild(d);
  }

  // mesa: a rodada que acabou de resolver + a rodada em curso
  const mesa = $('cartas-na-mesa');
  mesa.textContent = '';

  const ant = s.rodada_anterior;
  if (ant && ant.jogadas.length) {
    const bloco = document.createElement('div');
    bloco.className = 'bloco-anterior';
    const titulo = document.createElement('span');
    titulo.className = 'titulo-anterior';
    titulo.textContent =
      ant.vencedor === null
        ? `a ${ant.indice + 1}ª empatou`
        : `a ${ant.indice + 1}ª foi ${ant.vencedor === meuTime ? 'nossa' : 'deles'}`;
    bloco.appendChild(titulo);
    const fila = document.createElement('div');
    fila.className = 'cartas-anteriores';
    bloco.appendChild(fila);
    for (const j of ant.jogadas) {
      fila.appendChild(
        elNaMesa(j, {
          rankManilha,
          nomes,
          vencedores: ant.assentos_vencedores,
          houveVencedor: ant.vencedor !== null,
          pequena: true,
        })
      );
    }
    mesa.appendChild(bloco);
  }

  const atual = document.createElement('div');
  atual.className = 'bloco-atual';
  for (const c of s.mesa) {
    atual.appendChild(
      elNaMesa(c, { rankManilha, nomes, vencedores: [], houveVencedor: false, pequena: false })
    );
  }
  mesa.appendChild(atual);

  // as três rodadas
  const rod = $('rodadas');
  rod.textContent = '';
  for (let i = 0; i < 3; i++) {
    const r = s.rodadas[i];
    const e = document.createElement('div');
    const estadoR =
      r === undefined ? '' : r === null ? 'empate' : r === meuTime ? 'nossa' : 'deles';
    e.className = 'marca ' + estadoR;
    const n = document.createElement('span');
    n.className = 'n';
    n.textContent = `${i + 1}ª`;
    const q = document.createElement('span');
    q.textContent =
      r === undefined ? '—' : r === null ? 'empate' : r === meuTime ? 'nós' : 'eles';
    e.append(n, q);
    rod.appendChild(e);
  }

  // a sua mão, em leque
  const mao = $('minha-mao');
  mao.textContent = '';
  const podeJogar = s.opcoes.includes('jogar');
  const cartas = s.minha_mao || [];
  cartas.forEach((c, i) => {
    const b = document.createElement('button');
    b.className = 'carta' + (naipeDe(c) === 'copas' || naipeDe(c) === 'ouros' ? ' vermelha' : '');
    if (ehManilha(c, rankManilha)) b.classList.add('manilha');
    b.textContent = c;
    b.dataset.testid = `carta-${i}`;
    b.dataset.carta = c;
    b.disabled = !podeJogar;
    const meio = (cartas.length - 1) / 2;
    b.style.setProperty('--rot', `${(i - meio) * 7}deg`);
    b.style.setProperty('--sobe', `${Math.abs(i - meio) * 5}px`);
    b.title = `${rankDe(c)} de ${naipeDe(c)}${ehManilha(c, rankManilha) ? ' — manilha' : ''}`;
    b.setAttribute('aria-label', (podeJogar ? 'Jogar ' : '') + b.title);
    b.onclick = () => manda({ t: 'jogar', carta: c, encoberta: $('chk-encoberta').checked });
    mao.appendChild(b);
  });

  if (s.mao_do_parceiro) {
    const d = document.createElement('div');
    d.className = 'mao-parceiro';
    d.dataset.testid = 'mao-do-parceiro';
    const r = document.createElement('span');
    r.textContent = 'mão do parceiro:';
    d.appendChild(r);
    for (const c of s.mao_do_parceiro.cartas) {
      d.appendChild(elCarta(c, { manilha: ehManilha(c, rankManilha), classe: 'mini' }));
    }
    mao.appendChild(d);
  }

  $('encoberta-opcao').hidden = !(podeJogar && s.encoberta_permitida);
  if (!s.encoberta_permitida) $('chk-encoberta').checked = false;

  // ações
  const ligar = (id, ligado, texto) => {
    $(id).hidden = !ligado;
    if (texto) $(id).textContent = texto;
  };
  const proximo = { 1: 'TRUCO (3)', 3: 'seis', 6: 'nove', 9: 'doze' }[s.valor] || '';
  ligar('btn-pedir', s.opcoes.includes('pedir'), `pedir ${proximo}`);
  ligar('btn-aceitar', s.opcoes.includes('aceitar'), `Aceitar ${ultimoPedido ?? ''}`.trim());
  ligar('btn-correr', s.opcoes.includes('correr'), 'Correr');
  ligar('btn-aumentar', s.opcoes.includes('aumentar'), 'Aumentar');
  ligar('btn-onze-jogar', s.opcoes.includes('mao_de_onze_jogar'), 'Jogar a mão de onze');
  ligar('btn-onze-correr', s.opcoes.includes('mao_de_onze_correr'), 'Recusar e dar 1 a eles');

  const vez = $('vez');
  vez.classList.toggle('esperando', !s.pode_agir);
  if (s.pode_agir) {
    vez.textContent = s.opcoes.includes('mao_de_onze_jogar')
      ? 'Mão de onze: joga valendo 3 ou recusa?'
      : s.opcoes.includes('aceitar')
        ? `Pediram ${ultimoPedido ?? ''}. Corre, aceita ou sobe?`.replace('  ', ' ')
        : 'Sua vez. Escolha uma carta.';
  } else {
    vez.textContent =
      s.vez != null ? `Vez de ${s.jogadores[s.vez]?.apelido ?? '…'}` : 'Aguardando a mesa…';
  }
}

// ───────────────────────────────── eventos de UI

async function entrar(rota, corpo) {
  $('erro-entrada').textContent = '';
  try {
    const { jogador } = await api(rota, corpo ? { method: 'POST', body: JSON.stringify(corpo) } : { method: 'POST' });
    pintarPerfil(jogador);
    await conectar();
    await carregarLobby();
  } catch (e) {
    $('erro-entrada').textContent =
      {
        apelido_em_uso: 'Esse apelido já existe. Escolha outro.',
        senha_curta: 'A senha precisa de 6 caracteres ou mais.',
        apelido_invalido: 'Use de 2 a 20 letras, números, - ou _.',
        credenciais_invalidas: 'Apelido ou senha não conferem.',
      }[e.message] || e.message;
  }
}

$('btn-convidado').onclick = () => entrar('/api/convidado', null);
const credenciais = () => ({ apelido: $('campo-apelido').value, senha: $('campo-senha').value });
$('btn-cadastrar').onclick = () => entrar('/api/cadastro', credenciais());
$('btn-entrar').onclick = () => entrar('/api/entrar', credenciais());

$('btn-sair').onclick = async () => {
  await api('/api/sair', { method: 'POST' }).catch(() => {});
  location.reload();
};

for (const m of ['1x1', '2x2']) {
  $(`modo-${m}`).onclick = () => {
    modo = m;
    $('modo-1x1').classList.toggle('escolhido', m === '1x1');
    $('modo-2x2').classList.toggle('escolhido', m === '2x2');
  };
}

for (const b of document.querySelectorAll('.atalhos button')) {
  b.onclick = () => {
    const v = b.dataset.valor;
    $('campo-aposta').value = v === 'tudo' ? (meuPerfil?.saldo ?? 0) : v;
  };
}

$('btn-fila').onclick = () => {
  const bruta = Number($('campo-aposta').value);
  // Validação local só para dar erro rápido. A de verdade é no servidor, que lê o saldo do
  // banco — o cliente nunca é a autoridade sobre quanto alguém tem.
  if (!Number.isInteger(bruta) || bruta < 0) {
    $('status-fila').textContent = 'A aposta é um número inteiro, de 0 para cima.';
    return;
  }
  if (meuPerfil && bruta > meuPerfil.saldo) {
    $('status-fila').textContent = `Você tem ${meuPerfil.saldo} moedas.`;
    return;
  }
  $('status-fila').textContent = 'Entrando na fila…';
  manda({ t: 'entrar_fila', modo, aposta: bruta });
};

$('btn-pedir').onclick = () => manda({ t: 'pedir' });
$('btn-aceitar').onclick = () => manda({ t: 'aceitar' });
$('btn-correr').onclick = () => manda({ t: 'correr' });
$('btn-aumentar').onclick = () => manda({ t: 'aumentar' });
$('btn-onze-jogar').onclick = () => manda({ t: 'mao_de_onze', jogar: true });
$('btn-onze-correr').onclick = () => manda({ t: 'mao_de_onze', jogar: false });
$('btn-voltar').onclick = async () => {
  const { jogador } = await api('/api/eu');
  pintarPerfil(jogador);
  await carregarLobby();
};

$('btn-webhook').onclick = async () => {
  $('webhook-segredo').textContent = '';
  try {
    const r = await api('/api/webhooks', {
      method: 'POST',
      body: JSON.stringify({ url: $('webhook-url').value }),
    });
    $('webhook-segredo').textContent = `Guarde este segredo, ele só aparece agora: ${r.segredo}`;
    $('webhook-url').value = '';
    carregarWebhooks();
  } catch (e) {
    $('webhook-segredo').textContent = `Endereço recusado: ${e.message}`;
  }
};

// Sessão já existente (cookie): entra direto no lobby.
(async () => {
  try {
    const { jogador } = await api('/api/eu');
    pintarPerfil(jogador);
    await conectar();
    await carregarLobby();
  } catch (_) {
    mostrar('tela-entrada');
  }
})();
