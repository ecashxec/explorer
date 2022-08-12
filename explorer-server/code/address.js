const getAddress = () => window.location.pathname.split('/')[2];

function renderTxHashCoins(row) {
  return '<a href="/tx/' + row.txHash + '">' + 
  minifyBlockID(row.txHash) + ':' + row.outIdx +
    (row.isCoinbase ? '<div class="ui green horizontal label">Coinbase</div>' : '') +
    '</a>';
}

function renderRowsCoins(row, type, decimals, ticker) {
  if (type === 'token') {
    return ( 
      '<div class="coin-row">' +
      '<div>' + renderTxHashCoins(row) + '</div>' +
      '<div>' + '<a href="/block-height/' + row.blockHeight + '">' + renderInteger(row.blockHeight) + '</a>' + '</div>' +
      '<div>' + renderAmount(row.tokenAmount, decimals) + ' ' + ticker + '</div>' +
      '</div>'
      ); 
  }
  else return ( 
      '<div class="coin-row">' +
      '<div>' + renderTxHashCoins(row) + '</div>' +
      '<div>' + '<a href="/block-height/' + row.blockHeight + '">' + renderInteger(row.blockHeight) + '</a>' + '</div>' +
      '<div>' + renderSats(row.satsAmount) + ' XEC' + '</div>' +
      '</div>'
      ); 
}

var isSatsTableLoaded = false;
function loadSatsTable() {
  if (!isSatsTableLoaded) {
    for (let i = 0; i < addrBalances["main"].utxos.length; i++) {
      $('#sats-coins-table').append(renderRowsCoins(addrBalances["main"].utxos[i]));
    }
    isSatsTableLoaded = true;
  }
}

var isTokenTableLoaded = {};
function loadTokenTable(tokenId) {
  if (!isTokenTableLoaded[tokenId]) {
    for (let i = 0; i < addrBalances[tokenId].utxos.length; i++) {
      $("#tokens-coins-table-" + tokenId).append(renderRowsCoins(addrBalances[tokenId].utxos[i], 'token', addrBalances[tokenId].token?.decimals, addrBalances[tokenId].token?.tokenTicker));
    }
    isTokenTableLoaded[tokenId] = true;
  }
}

const renderAge = timestamp => {
  if (timestamp == 0) {
    return '<div class="ui gray horizontal label">Mempool</div>';
  }
  return moment(timestamp * 1000).fromNow();
};

const renderTimestamp = timestamp => {
  if (timestamp == 0) {
    return '<div class="ui gray horizontal label">Mempool</div>';
  }
  return moment(timestamp * 1000).format('ll, LTS');
};

const renderTxID = (data) => {
  if (data.blockHeight === 0) {
  return '<a style="color:#CD0BC3" href="/tx/' + data.txHash + '">' + renderTxHash(data.txHash) + '</a>';
  }
  else {
    return '<a href="/tx/' + data.txHash + '">' + renderTxHash(data.txHash) + '</a>';
  }
};

const renderBlockHeight = (_value, _type, row) => {
  if (row.timestamp == 0) {
    return '<div class="ui gray horizontal label">Mempool</div>';
  }
  return '<a href="/block-height/' + row.blockHeight + '">' + renderInteger(row.blockHeight) + '</a>';
};

const renderSize = size => formatByteSize(size);

const renderFee = (_value, _type, row) => {
  if (row.isCoinbase) {
    return '<div class="ui green horizontal label">Coinbase</div>';
  }

  const fee = renderInteger((row.stats.satsInput - row.stats.satsOutput) / 100);
  let markup = '';

  markup += `<span>${fee}</span>`
  markup += `<span class="fee-per-byte">&nbsp(${renderFeePerByte(_value, _type, row)})</span>`

  return markup;
};

const renderFeePerByte = (_value, _type, row) => {
  if (row.isCoinbase) {
    return '';
  }
  const fee = row.stats.satsInput - row.stats.satsOutput;
  const feePerByte = fee / row.size;
  return renderInteger(Math.round(feePerByte * 1000)) + '/kB';
};

const renderAmountXEC = (_value, _type, row) => renderSats(row.stats.deltaSats) + ' XEC';

const renderToken = (_value, _type, row) => {
  if (row.token !== null) {
    var ticker = ' <a href="/tx/' + row.token.tokenId + '">' + row.token.tokenTicker + '</a>';
    return renderAmount(row.stats.deltaTokens, row.token.decimals) + ticker;
  }
  return '';
};

const updateTableLoading = (isLoading, tableId) => {
  if (isLoading) {
    $(`#${tableId} > tbody`).addClass('blur');
    $('#pagination').addClass('hidden');
    $('#footer').addClass('hidden');
  } else {
    $(`#${tableId} > tbody`).removeClass('blur');
    $('#pagination').removeClass('hidden');
    $('#footer').removeClass('hidden');
  }
};

const datatable = () => {
  const address = getAddress();

  $('#address-txs-table').DataTable({
    searching: false,
    lengthMenu: [50, 100, 200],
    pageLength: DEFAULT_ROWS_PER_PAGE,
    language: {
      loadingRecords: '',
      zeroRecords: '',
      emptyTable: '',
    },
    ajax: `/api/address/${address}/transactions`,
    order: [],
    responsive: {
        details: {
            type: 'column',
            target: -1
        }
    },
    columnDefs: [ {
        className: 'dtr-control',
        orderable: false,
        targets:   -1
    } ],
    columns:[
      { name: "age", data: 'timestamp', title: "Age", render: renderAge },
      { name: "timestamp", data: 'timestamp', title: "Date (UTC" + tzOffset + ")", render: renderTimestamp },
      { name: "txHash", data: {txHash: 'txHash', blockHeight: 'blockHeight'}, title: "Transaction ID", className: "hash", render: renderTxID },
      { name: "blockHeight", title: "Block Height", render: renderBlockHeight },
      { name: "size", data: 'size', title: "Size", render: renderSize },
      { name: "fee", title: "Fee", className: "fee", render: renderFee },
      { name: "numInputs", data: 'numInputs', title: "Inputs" },
      { name: "numOutputs", data: 'numOutputs', title: "Outputs" },
      { name: "deltaSats", data: 'deltaSats', title: "Amount XEC", render: renderAmountXEC },
      { name: "token", title: "Amount Token", render: renderToken },
      { name: 'responsive', render: () => '' },
    ],
  });
}

$('#address-txs-table').on('xhr.dt', () => {
  updateTableLoading(false, 'address-txs-table');
});

const updateTable = (paginationRequest) => {
  const params = new URLSearchParams(paginationRequest).toString();
  const address = getAddress();

  updateTableLoading(true, 'address-txs-table');
  $('#address-txs-table').dataTable().api().ajax.url(`/api/address/${address}/transactions?${params}`).load()
}

const goToPage = (event, page) => {
  event.preventDefault();
  reRenderPage({ page });
};

$(document).on('change', '[name="address-txs-table_length"]', event => {
  reRenderPage({ rows: event.target.value, page: 1 });
});

const reRenderPage = params => {
  if (params) {
    window.state.updateParameters(params)
  }

  const paginationRequest = window.pagination.generatePaginationRequest();
  updateTable(paginationRequest);

  const { currentPage, pageArray } = window.pagination.generatePaginationUIParams();
  window.pagination.generatePaginationUI(currentPage, pageArray);
};

$(document).ready(() => {
  datatable();
  reRenderPage();
});
