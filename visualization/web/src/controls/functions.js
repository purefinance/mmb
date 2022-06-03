function groupBy(list, keyGetter) {
  let collection = {};
  list.forEach((item) => {
    const key = keyGetter(item);
    if (!collection[key]) {
      collection[key] = { exchanges: [item] };
    } else {
      collection[key].exchanges.push(item);
    }
  });
  return collection;
}

function flatten(arr) {
  return arr.reduce(function (flat, toFlatten) {
    return flat.concat(
      Array.isArray(toFlatten) ? flatten(toFlatten) : toFlatten
    );
  }, []);
}

function orderAscending(arr) {
  var ordered = Object.keys(arr)
    .map(function (sortedKey) {
      return [Number(sortedKey), arr[sortedKey]];
    })
    .sort(function (a, b) {
      return a[0] - b[0];
    });
  return ordered;
}

function orderDescending(arr) {
  var ordered = Object.keys(arr)
    .map(function (sortedKey) {
      return [Number(sortedKey), arr[sortedKey]];
    })
    .sort(function (a, b) {
      return b[0] - a[0];
    });
  return ordered;
}

//https://dev.to/saigowthamr/how-to-remove-duplicate-objects-from-an-array-javascript-48ok
function removeDuplicates(arr, comp) {
  const unique = arr
    .map((e) => e[comp])
    // store the keys of the unique objects
    .map((e, i, final) => final.indexOf(e) === i && i)
    // eliminate the dead keys & store unique objects
    .filter((e) => arr[e])
    .map((e) => arr[e]);

  return unique;
}

function orderByDate(arr) {
  return arr.sort(function (a, b) {
    var dateA = new Date(a.dateTime),
      dateB = new Date(b.dateTime);
    return dateB - dateA;
  });
}

export {
  groupBy,
  flatten,
  orderAscending,
  orderDescending,
  removeDuplicates,
  orderByDate,
};
