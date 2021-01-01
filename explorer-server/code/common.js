var tzOffset;

{
  var offsetMinutes = new Date().getTimezoneOffset();
  if (offsetMinutes < 0) {
    tzOffset = moment.utc(moment.duration(-offsetMinutes, 'minutes').asMilliseconds()).format('+HH:mm');
  } else {
    tzOffset = moment.utc(moment.duration(offsetMinutes, 'minutes').asMilliseconds()).format('-HH:mm');
  }
}

function formatByteSize(size) {
  if (size < 1024) {
    return size + ' B';
  } else if (size < 1024 * 1024) {
    return (size / 1000).toFixed(2) + ' kB';
  } else {
    return (size / 1000000).toFixed(2) + ' MB';
  }
}

function renderInteger(number) {
  var fmt = Intl.NumberFormat('en-EN').format(number);
  var parts = fmt.split(',');
  var str = '';
  for (var i = 0; i < parts.length - 1; ++i) {
    str += '<span class="digit-sep">' + parts[i] + '</span>';
  }
  str += '<span>' + parts[parts.length - 1] + '</span>';
  return str;
}

function renderAmount(baseAmount, decimals) {
  if (decimals === 0) {
    return renderInteger(baseAmount)
  }
  var factor = Math.pow(10, decimals);
  var humanAmount = baseAmount / factor;
  var fmt = humanAmount.toFixed(decimals);
  var parts = fmt.split('.');
  var integerPart = parseInt(parts[0]);
  var fractPart = parts[1];
  var numFractSections = Math.ceil(decimals / 3);
  var fractRendered = '';
  var allZeros = true;
  for (var sectionIdx = numFractSections - 1; sectionIdx >= 0; --sectionIdx) {
    var section = fractPart.substr(sectionIdx * 3, 3);
    if (parseInt(section) !== 0)
      allZeros = false;
    var classes =
      (allZeros ? 'zeros ' : '') +
      (sectionIdx != numFractSections - 1 ? 'digit-sep ' : '');
    fractRendered = '<small class="' + classes + '">' + section + '</small>' + fractRendered;
  }
  return renderInteger(integerPart) + '.' + fractRendered;
}

function renderSats(sats) {
  var coins = sats / 100000000;
  var fmt = coins.toFixed('8');
  var parts = fmt.split('.');
  var integerPart = parseInt(parts[0]);
  var fractPart = parts[1];
  var fract1 = fractPart.substr(0, 3);
  var fract2 = fractPart.substr(3, 3);
  var fract3 = fractPart.substr(6, 2);
  var z1 = fract1 === '000';
  var z2 = fract2 === '000';
  var z3 = fract3 === '00';
  var renderedFract1 = z1 && z2 && z3
    ? '<span class="zeros digit-sep">' + fract1 + '</span>'
    : '<span class="digit-sep">' + fract1 + '</span>';
  var renderedFract2 = z1 && z2
    ? '<small class="zeros digit-sep">' + fract2 + '</small>'
    : '<small class="digit-sep">' + fract2 + '</small>';
  var renderedFract3 = z3
    ? '<small class="zeros digit-sep">' + fract3 + '</small>'
    : '<small class="digit-sep">' + fract3 + '</small>';
  if (coins < 100) {
    return renderInteger(integerPart) + '.' + renderedFract1 + renderedFract2 + renderedFract3;
  } else if (coins < 10000) {
    return renderInteger(integerPart) + '.' + renderedFract1 + renderedFract2;
  } else {
    return renderInteger(integerPart) + '.' + renderedFract1;
  }
}
