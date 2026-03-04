

// Using iframe to load the exact HTML safely while retaining CSS within the React App.
const HTMLViewer = ({ filename }: { filename: string }) => {
  return (
    <div style={{ width: '100%', height: '100%' }}>
      <iframe
        src={`/assets/${filename}`}
        style={{ width: '100%', height: '100%', border: 'none' }}
        title={filename}
      />
    </div>
  );
};

export default HTMLViewer;
